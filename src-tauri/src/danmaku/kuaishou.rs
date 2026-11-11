//! 快手弹幕接入（只读）
//!
//! 快手 `websocketinfo` 接口有风控：未登录会返回滑块验证（400002 / result=2），
//! 因此必须先通过设置中心完成快手登录，弹幕模块再复用已保存的会话 cookie 换 token。
//!
//! 协议：
//! - GET `live_api/liveroom/websocketinfo?caver=2&liveStreamId=...` → token + webSocketUrls；
//! - WebSocket 发送 SocketMessage{payloadType=200, payload=CsWebEnterRoom{token,liveStreamId,pageId}}；
//! - 每 20 秒发送 SocketMessage{payloadType=1, payload=CsHeartbeat{timestamp}}；
//! - 下行 SocketMessage{payloadType=310} → ScWebFeedPush → commentFeeds（protobuf）。

use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context};
use flate2::read::GzDecoder;
use futures_util::{SinkExt, StreamExt};
use tauri::AppHandle;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request as WsRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use super::proto::{self, Field};
use super::DanmakuMessage;

const PLATFORM: &str = "kuaishou";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const WEBSOCKET_INFO_URL: &str = "https://live.kuaishou.com/live_api/liveroom/websocketinfo";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);
const PAYLOAD_CS_HEARTBEAT: u64 = 1;
const PAYLOAD_CS_ENTER_ROOM: u64 = 200;
const PAYLOAD_SC_FEED_PUSH: u64 = 310;
const COMPRESSION_GZIP: u64 = 2;

struct WsInfo {
    token: String,
    urls: Vec<String>,
    live_stream_id: String,
}

fn cookie_value(cookie: &str, name: &str) -> Option<String> {
    cookie.split(';').find_map(|part| {
        let part = part.trim();
        let (key, value) = part.split_once('=')?;
        (key.trim() == name).then(|| value.trim().to_string())
    })
}

fn extract_kww(cookie: &str) -> Option<String> {
    cookie_value(cookie, "kwfv1").or_else(|| cookie_value(cookie, "kww"))
}

async fn fetch_ws_info(
    client: &reqwest::Client,
    live_stream_id: &str,
    principal_id: &str,
    cookie: &str,
) -> anyhow::Result<WsInfo> {
    let mut request = client
        .get(WEBSOCKET_INFO_URL)
        .query(&[("caver", "2"), ("liveStreamId", live_stream_id)])
        .header("User-Agent", USER_AGENT)
        .header(
            "Referer",
            format!("https://live.kuaishou.com/u/{principal_id}"),
        )
        .header("Cookie", cookie);
    if let Some(kww) = extract_kww(cookie) {
        request = request.header("Kww", kww);
    }

    let json: serde_json::Value = request
        .send()
        .await
        .context("快手弹幕握手信息请求失败")?
        .error_for_status()
        .context("快手弹幕握手信息返回异常状态")?
        .json()
        .await
        .context("快手弹幕握手信息解析失败")?;

    let data = json.get("data").unwrap_or(&serde_json::Value::Null);
    let result = data.get("result").and_then(|v| v.as_i64()).unwrap_or(1);
    if result != 1 {
        let msg = data
            .get("error_msg")
            .and_then(|v| v.as_str())
            .unwrap_or("需要人机验证或登录已失效");
        return Err(anyhow!(
            "快手弹幕握手被风控拦截（result={}）：{}；请在设置中心重新登录快手后重试",
            result,
            msg
        ));
    }

    let token = data
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("快手未返回弹幕 token（登录态可能已失效）"))?
        .to_string();

    let mut urls = Vec::new();
    for key in ["websocketUrls", "webSocketUrls", "webSocketAddresses"] {
        if let Some(list) = data.get(key).and_then(|v| v.as_array()) {
            for item in list {
                if let Some(url) = item.as_str() {
                    if url.starts_with("wss://") || url.starts_with("ws://") {
                        urls.push(url.to_string());
                    }
                }
            }
        }
    }
    if urls.is_empty() {
        return Err(anyhow!("快手未返回弹幕 WebSocket 地址"));
    }

    let live_stream_id = data
        .get("liveStreamId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(live_stream_id)
        .to_string();

    Ok(WsInfo {
        token,
        urls,
        live_stream_id,
    })
}

fn build_ws_request(url: &str, cookie: &str) -> anyhow::Result<WsRequest> {
    let mut request = url.into_client_request().context("构造快手弹幕握手请求失败")?;
    let headers = request.headers_mut();
    headers.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
    headers.insert("Origin", HeaderValue::from_static("https://live.kuaishou.com"));
    if !cookie.is_empty() {
        headers.insert(
            "Cookie",
            HeaderValue::from_str(cookie).context("快手 cookie 不是合法 Header")?,
        );
    }
    Ok(request)
}

fn page_id() -> String {
    const CHARS: &[u8] = b"bjectSymhasOwnProp-0123456789ABCDEFGHIJKLMNQRTUVWXYZ_dfgiklquvxz";
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let prefix: String = (0..16)
        .map(|i| CHARS[bytes[i] as usize % CHARS.len()] as char)
        .collect();
    format!("{}_{}", prefix, chrono::Utc::now().timestamp_millis())
}

fn enter_room_frame(info: &WsInfo) -> Vec<u8> {
    let mut payload = proto::field_string(1, &info.token);
    payload.extend_from_slice(&proto::field_string(2, &info.live_stream_id));
    payload.extend_from_slice(&proto::field_string(7, &page_id()));
    let mut frame = proto::field_varint(1, PAYLOAD_CS_ENTER_ROOM);
    frame.extend_from_slice(&proto::field_bytes(3, &payload));
    frame
}

fn heartbeat_frame() -> Vec<u8> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let payload = proto::field_varint(1, timestamp);
    let mut frame = proto::field_varint(1, PAYLOAD_CS_HEARTBEAT);
    frame.extend_from_slice(&proto::field_bytes(3, &payload));
    frame
}

fn parse_socket_message(data: &[u8]) -> Option<(u64, u64, Vec<u8>)> {
    let fields = proto::parse_fields(data);
    let payload_type = proto::first_varint(&fields, 1)?;
    let compression = proto::first_varint(&fields, 2).unwrap_or(0);
    let payload = proto::first_bytes(&fields, 3)?.to_vec();
    Some((payload_type, compression, payload))
}

fn normalize_color(raw: &str) -> Option<String> {
    let hex = raw.trim().trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{}", hex.to_uppercase()))
    } else {
        None
    }
}

fn parse_comment_feeds(payload: &[u8]) -> Vec<(String, String, Option<String>)> {
    let feed = proto::parse_fields(payload);
    let mut out = Vec::new();
    for (field, value) in &feed {
        if *field != 5 {
            continue;
        }
        let Field::Bytes(comment) = value else {
            continue;
        };
        let comment = proto::parse_fields(comment);
        let content = proto::first_string(&comment, 3).unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        let username = proto::first_bytes(&comment, 2)
            .map(proto::parse_fields)
            .and_then(|user| proto::first_string(&user, 2))
            .unwrap_or_default();
        let color = proto::first_string(&comment, 6).and_then(|c| normalize_color(&c));
        out.push((username, content, color));
    }
    out
}

pub async fn run(app: &AppHandle, room_id: &str, tx: Sender<DanmakuMessage>) -> anyhow::Result<()> {
    log::info!("[danmaku] kuaishou 开始连接房间 {}", room_id);
    let cookie = crate::auth::platform_cookie(PLATFORM).ok_or_else(|| {
        anyhow!("快手弹幕需要登录：请在设置中心打开快手登录窗口完成登录后重试")
    })?;

    let adapter = crate::platforms::kuaishou::KuaishouAdapter::new();
    let principal_id = adapter
        .lookup_principal(room_id)
        .await
        .unwrap_or_else(|| room_id.to_string());
    log::info!(
        "[danmaku] kuaishou 房间 {} 对应主播 {}",
        room_id,
        principal_id
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .context("创建快手弹幕 HTTP 客户端失败")?;
    let info = fetch_ws_info(&client, room_id, &principal_id, &cookie).await?;
    let ws_url = info
        .urls
        .first()
        .cloned()
        .ok_or_else(|| anyhow!("快手未返回可用的弹幕服务器"))?;
    let request = build_ws_request(&ws_url, &cookie)?;

    let (mut ws, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .context("连接快手弹幕服务器超时")?
    .context("连接快手弹幕服务器失败")?;

    ws.send(Message::Binary(enter_room_frame(&info).into()))
        .await
        .context("发送快手弹幕进房包失败")?;
    super::emit_status(app, PLATFORM, room_id, "connected", None);
    log::info!("[danmaku] kuaishou 连接成功");

    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                ws.send(Message::Binary(heartbeat_frame().into()))
                    .await
                    .context("发送快手弹幕心跳失败")?;
            }
            frame = ws.next() => {
                match frame {
                    Some(Ok(Message::Binary(data))) => {
                        let Some((payload_type, compression, payload)) = parse_socket_message(&data) else {
                            continue;
                        };
                        if payload_type != PAYLOAD_SC_FEED_PUSH {
                            continue;
                        }
                        let payload = if compression == COMPRESSION_GZIP {
                            let mut out = Vec::new();
                            if GzDecoder::new(&payload[..]).read_to_end(&mut out).is_err() {
                                continue;
                            }
                            out
                        } else {
                            payload
                        };
                        for (username, text, color) in parse_comment_feeds(&payload) {
                            if tx
                                .send(super::message(PLATFORM, room_id, username, text, color))
                                .await
                                .is_err()
                            {
                                return Ok(());
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        ws.send(Message::Pong(payload))
                            .await
                            .context("回复快手弹幕 Ping 失败")?;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(anyhow!("快手弹幕连接已被服务器关闭"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::Error::from(e).context("快手弹幕连接异常"));
                    }
                    None => return Err(anyhow!("快手弹幕连接结束")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_room_frame_is_valid_protobuf() {
        let info = WsInfo {
            token: "token".to_string(),
            urls: vec![],
            live_stream_id: "abc123".to_string(),
        };
        let frame = enter_room_frame(&info);
        let fields = proto::parse_fields(&frame);
        assert_eq!(proto::first_varint(&fields, 1), Some(PAYLOAD_CS_ENTER_ROOM));
        let payload = proto::first_bytes(&fields, 3).expect("payload");
        let payload = proto::parse_fields(payload);
        assert_eq!(proto::first_string(&payload, 1).as_deref(), Some("token"));
        assert_eq!(proto::first_string(&payload, 2).as_deref(), Some("abc123"));
    }

    #[test]
    fn color_normalize() {
        assert_eq!(normalize_color("#ff00aa").as_deref(), Some("#FF00AA"));
        assert_eq!(normalize_color("FFFFFF").as_deref(), Some("#FFFFFF"));
        assert_eq!(normalize_color(""), None);
    }
}
