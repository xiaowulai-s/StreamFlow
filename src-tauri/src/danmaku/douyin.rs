//! 抖音弹幕接入（只读）
//!
//! 协议（2026-11 实测）：
//! - WebSocket：`wss://webcast100-ws-web-lq.douyin.com/webcast/im/push/v2/`
//! - 必须携带 `ttwid` 访客 Cookie + `signature`（X-Bogus 变体）才能完成握手；
//! - 进房：连接后发送 `PushFrame{ payloadType: "hb" }`；
//! - 下行：PushFrame(protobuf) → gzip 解压 Response → Message → WebcastChatMessage；
//! - 服务端 needAck 时回 `PushFrame{ logId, payloadType: internalExt }`；
//! - 每 10 秒发送一次 hb 心跳。

use std::io::Read;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

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

const PLATFORM: &str = "douyin";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36 Edg/125.0.0.0";
const WS_URL: &str = "wss://webcast100-ws-web-lq.douyin.com/webcast/im/push/v2/";
const VERSION_CODE: &str = "180800";
const SDK_VERSION: &str = "1.0.14-beta.0";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const XBOGUS_ALPHABET: &str = "Dkdpgh4ZKsQB80/Mfvw36XI1R25+WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe";

/// ttwid 访客 Cookie 缓存（1 小时）
static TTWID_CACHE: OnceLock<tokio::sync::Mutex<Option<(Instant, String)>>> = OnceLock::new();

fn ttwid_cache() -> &'static tokio::sync::Mutex<Option<(Instant, String)>> {
    TTWID_CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
}

/// 抓取 live.douyin.com 首页下发的 ttwid；
/// 没有它 WebSocket 握手会被返回 HTTP 200 空响应。
async fn fetch_ttwid(client: &reqwest::Client) -> anyhow::Result<String> {
    let mut guard = ttwid_cache().lock().await;
    if let Some((at, value)) = guard.as_ref() {
        if at.elapsed() < Duration::from_secs(3600) {
            return Ok(value.clone());
        }
    }

    let resp = client
        .get("https://live.douyin.com/")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .context("获取抖音访客 Cookie 失败")?;
    let mut ttwid = None;
    for value in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        let Ok(raw) = value.to_str() else { continue };
        let pair = raw.split(';').next().unwrap_or("");
        if let Some((name, val)) = pair.split_once('=') {
            if name.trim() == "ttwid" && !val.trim().is_empty() {
                ttwid = Some(val.trim().to_string());
            }
        }
    }
    let ttwid = ttwid.ok_or_else(|| anyhow!("抖音未下发 ttwid Cookie（可能触发风控）"))?;
    *guard = Some((Instant::now(), ttwid.clone()));
    Ok(ttwid)
}

fn random_user_id() -> String {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[..8]);
    let value = u64::from_le_bytes(raw) % 9_000_000_000_000 + 1_000_000_000_000;
    value.to_string()
}

fn md5_hex(input: &str) -> String {
    hex::encode(md5::compute(input.as_bytes()).0)
}

fn rc4(key: u8, data: &mut [u8]) {
    let mut s = [0u8; 256];
    for (i, slot) in s.iter_mut().enumerate() {
        *slot = i as u8;
    }
    let mut j = 0usize;
    for i in 0..256 {
        j = (j + s[i] as usize + key as usize) & 0xff;
        s.swap(i, j);
    }
    let mut i = 0usize;
    j = 0;
    for byte in data.iter_mut() {
        i = (i + 1) & 0xff;
        j = (j + s[i] as usize) & 0xff;
        s.swap(i, j);
        *byte ^= s[(s[i] as usize + s[j] as usize) & 0xff];
    }
}

/// X-Bogus 自定义 base64（12 字节 → 16 字符）
fn encode_xbogus(data: &[u8]) -> String {
    let alphabet = XBOGUS_ALPHABET.as_bytes();
    let mut out = String::new();
    for chunk in data.chunks_exact(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk[1] as usize;
        let b2 = chunk[2] as usize;
        out.push(alphabet[(b0 >> 2) & 0x3f] as char);
        out.push(alphabet[((b0 << 4) | (b1 >> 4)) & 0x3f] as char);
        out.push(alphabet[((b1 << 2) | (b2 >> 6)) & 0x3f] as char);
        out.push(alphabet[b2 & 0x3f] as char);
    }
    out
}

/// 生成抖音 WebSocket 的 signature（移植 stream-rec / pure_live 的 X-Bogus 实现）
fn xbogus(ms_stub: &str, counter: u8) -> String {
    let random_bytes = uuid::Uuid::new_v4().into_bytes();
    let random1 = random_bytes[0];
    let random2 = random_bytes[1] % 255;
    let header = 0x40 | (random1 & 0x1f);

    let Ok(stub) = hex::decode(ms_stub) else {
        return String::new();
    };
    let digest = md5::compute(stub);
    let mut payload = [
        counter & 0x3f,
        0,
        1,
        0x0e,
        0x45,
        0x3f,
        digest[14],
        digest[15],
        random2,
        0,
    ];
    let mut checksum = 0u8;
    for byte in payload.iter().take(9) {
        checksum ^= *byte;
    }
    payload[9] = checksum;
    rc4(random2, &mut payload);

    let mut final_data = [0u8; 12];
    final_data[0] = header;
    final_data[1] = random2;
    final_data[2..].copy_from_slice(&payload);
    encode_xbogus(&final_data)
}

fn build_signature(room_id: &str, user_id: &str) -> String {
    let raw = format!(
        "live_id=1,aid=6383,version_code={VERSION_CODE},webcast_sdk_version={SDK_VERSION},\
         room_id={room_id},sub_room_id=,sub_channel_id=,did_rule=3,\
         user_unique_id={user_id},device_platform=web,device_type=,ac=,identity=audience"
    );
    xbogus(&md5_hex(&raw), 1)
}

fn build_ws_url(room_id: &str, user_id: &str) -> String {
    let mut url = reqwest::Url::parse(WS_URL).expect("抖音 WebSocket URL 固定合法");
    {
        let mut query = url.query_pairs_mut();
        for (key, value) in [
            ("app_name", "douyin_web".to_string()),
            ("version_code", VERSION_CODE.to_string()),
            ("webcast_sdk_version", SDK_VERSION.to_string()),
            ("update_version_code", SDK_VERSION.to_string()),
            ("compress", "gzip".to_string()),
            (
                "cursor",
                format!("h-1_t-{}_r-1_d-1_u-1", chrono::Utc::now().timestamp_millis()),
            ),
            ("host", "https://live.douyin.com".to_string()),
            ("aid", "6383".to_string()),
            ("live_id", "1".to_string()),
            ("did_rule", "3".to_string()),
            ("debug", "false".to_string()),
            ("maxCacheMessageNumber", "20".to_string()),
            ("endpoint", "live_pc".to_string()),
            ("support_wrds", "1".to_string()),
            ("im_path", "/webcast/im/fetch/".to_string()),
            ("user_unique_id", user_id.to_string()),
            ("device_platform", "web".to_string()),
            ("cookie_enabled", "true".to_string()),
            ("screen_width", "1920".to_string()),
            ("screen_height", "1080".to_string()),
            ("browser_language", "zh-CN".to_string()),
            ("browser_platform", "Win32".to_string()),
            ("browser_name", "Mozilla".to_string()),
            (
                "browser_version",
                USER_AGENT.trim_start_matches("Mozilla/").to_string(),
            ),
            ("browser_online", "true".to_string()),
            ("tz_name", "Asia/Shanghai".to_string()),
            ("identity", "audience".to_string()),
            ("room_id", room_id.to_string()),
            ("heartbeatDuration", "0".to_string()),
        ] {
            query.append_pair(key, &value);
        }
    }
    let signature = build_signature(room_id, user_id);
    format!("{url}&signature={signature}")
}

fn build_ws_request(url: &str, ttwid: &str) -> anyhow::Result<WsRequest> {
    let mut request = url.into_client_request().context("构造抖音弹幕握手请求失败")?;
    let headers = request.headers_mut();
    headers.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
    headers.insert("Origin", HeaderValue::from_static("https://live.douyin.com"));
    headers.insert(
        "Cookie",
        HeaderValue::from_str(&format!("ttwid={ttwid}")).context("ttwid 不是合法 Header")?,
    );
    Ok(request)
}

fn heartbeat_frame() -> Vec<u8> {
    proto::field_string(7, "hb")
}

fn ack_frame(log_id: u64, internal_ext: &str) -> Vec<u8> {
    let mut out = proto::field_varint(2, log_id);
    let payload_type = if internal_ext.is_empty() {
        "ack"
    } else {
        internal_ext
    };
    out.extend_from_slice(&proto::field_string(7, payload_type));
    out
}

struct ParsedFrame {
    ack: Option<(u64, String)>,
    chats: Vec<(String, String)>,
}

fn parse_message(payload: &[u8]) -> Option<(String, String)> {
    let message = proto::parse_fields(payload);
    let method = proto::first_string(&message, 1)?;
    if method != "WebcastChatMessage" {
        return None;
    }
    let body = proto::first_bytes(&message, 2)?;
    let chat = proto::parse_fields(body);
    let content = proto::first_string(&chat, 3)?;
    if content.is_empty() {
        return None;
    }
    let username = proto::first_bytes(&chat, 2)
        .map(proto::parse_fields)
        .and_then(|user| proto::first_string(&user, 3))
        .unwrap_or_default();
    Some((username, content))
}

fn parse_frame(data: &[u8]) -> ParsedFrame {
    let frame = proto::parse_fields(data);
    let log_id = proto::first_varint(&frame, 2).unwrap_or(0);
    let Some(payload) = proto::first_bytes(&frame, 8) else {
        return ParsedFrame {
            ack: None,
            chats: Vec::new(),
        };
    };
    let mut decompressed = Vec::new();
    if GzDecoder::new(payload)
        .read_to_end(&mut decompressed)
        .is_err()
    {
        return ParsedFrame {
            ack: None,
            chats: Vec::new(),
        };
    }

    let response = proto::parse_fields(&decompressed);
    let mut internal_ext = String::new();
    let mut need_ack = false;
    let mut chats = Vec::new();
    for (field, value) in &response {
        match (field, value) {
            (1, Field::Bytes(message)) => {
                if let Some(chat) = parse_message(message) {
                    chats.push(chat);
                }
            }
            (5, Field::Bytes(bytes)) => {
                internal_ext = String::from_utf8_lossy(bytes).to_string();
            }
            (9, Field::Varint(value)) => need_ack = *value == 1,
            _ => {}
        }
    }

    ParsedFrame {
        ack: need_ack.then_some((log_id, internal_ext)),
        chats,
    }
}

pub async fn run(app: &AppHandle, room_id: &str, tx: Sender<DanmakuMessage>) -> anyhow::Result<()> {
    log::info!("[danmaku] douyin 开始连接房间 {}", room_id);
    let adapter = crate::platforms::douyin::DouyinAdapter::new();
    let real_room = adapter.resolve_room_id(room_id).await?;
    log::info!("[danmaku] douyin 真实房间号 {}", real_room);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .context("创建抖音弹幕 HTTP 客户端失败")?;
    let ttwid = fetch_ttwid(&client).await?;
    let user_id = random_user_id();
    let url = build_ws_url(&real_room, &user_id);
    let request = build_ws_request(&url, &ttwid)?;

    let (mut ws, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .context("连接抖音弹幕服务器超时")?
    .context("连接抖音弹幕服务器失败（签名或访客 Cookie 被拒绝）")?;

    ws.send(Message::Binary(heartbeat_frame().into()))
        .await
        .context("发送抖音弹幕进房包失败")?;
    super::emit_status(app, PLATFORM, room_id, "connected", None);
    log::info!("[danmaku] douyin 连接成功");

    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                ws.send(Message::Binary(heartbeat_frame().into()))
                    .await
                    .context("发送抖音弹幕心跳失败")?;
            }
            frame = ws.next() => {
                match frame {
                    Some(Ok(Message::Binary(data))) => {
                        let parsed = parse_frame(&data);
                        if let Some((log_id, internal_ext)) = parsed.ack {
                            ws.send(Message::Binary(ack_frame(log_id, &internal_ext).into()))
                                .await
                                .context("发送抖音弹幕 ACK 失败")?;
                        }
                        for (username, text) in parsed.chats {
                            if tx
                                .send(super::message(PLATFORM, room_id, username, text, None))
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
                            .context("回复抖音弹幕 Ping 失败")?;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(anyhow!("抖音弹幕连接已被服务器关闭"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::Error::from(e).context("抖音弹幕连接异常"));
                    }
                    None => return Err(anyhow!("抖音弹幕连接结束")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_16_chars() {
        let signature = build_signature("7706776263012207410", "1234567890123");
        assert_eq!(signature.len(), 16, "signature: {signature}");
    }

    #[test]
    fn parse_heartbeat_frame() {
        let frame = heartbeat_frame();
        let fields = proto::parse_fields(&frame);
        assert_eq!(proto::first_string(&fields, 7).as_deref(), Some("hb"));
    }
}
