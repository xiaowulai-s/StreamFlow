//! B站直播弹幕接入（只读）
//!
//! 流程：先用 `room/v1/Room/get_info` 把短号换成真实房间号，再连接弹幕
//! WebSocket，发送认证包后按 16 字节包头的二进制协议解析下行帧；
//! ver=2 的帧是 zlib 压缩的多帧数据，解压后递归解析。

use std::io::Read;
use std::time::Duration;

use anyhow::{anyhow, Context};
use flate2::read::ZlibDecoder;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request as WsRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use super::DanmakuMessage;

const PLATFORM: &str = "bilibili";
const ROOM_INFO_URL: &str = "https://api.live.bilibili.com/room/v1/Room/get_info";
const CONF_URL: &str = "https://api.live.bilibili.com/room/v1/Danmu/getConf";
const WS_URL: &str = "wss://broadcastlv.chat.bilibili.com/sub";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// 心跳间隔（协议要求 30 秒）
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
/// 操作码：心跳 / 认证
const OP_HEARTBEAT: u32 = 2;
const OP_AUTH: u32 = 7;
/// 压缩帧递归解析的最大深度，防御异常或恶意嵌套
const MAX_DEPTH: u8 = 8;

/// 组装二进制包：16 字节包头（包长 u32、头长 u16、ver u16、op u32、seq u32）+ body。
fn build_packet(body: &[u8], op: u32) -> Vec<u8> {
    let total = 16 + body.len();
    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&(total as u32).to_be_bytes()); // 包总长
    buf.extend_from_slice(&16u16.to_be_bytes()); // 包头长度
    buf.extend_from_slice(&1u16.to_be_bytes()); // 协议版本
    buf.extend_from_slice(&op.to_be_bytes()); // 操作码
    buf.extend_from_slice(&1u32.to_be_bytes()); // 序号
    buf.extend_from_slice(body);
    buf
}

/// 查询真实房间号；接口失败或返回异常时返回 None（调用方回退到入参）。
async fn fetch_real_room_id(room_id: &str) -> Option<String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(ROOM_INFO_URL)
        .query(&[("room_id", room_id)])
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    let json: Value = resp.json().await.ok()?;
    if json.get("code").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    let id = json.get("data")?.get("room_id")?;
    id.as_u64()
        .map(|v| v.to_string())
        .or_else(|| id.as_str().map(str::to_string))
}

/// 构造带 UA / Origin 的 WebSocket 握手请求。
fn build_ws_request(url: &str) -> anyhow::Result<WsRequest> {
    let mut request = url
        .into_client_request()
        .context("构造 B站弹幕连接请求失败")?;
    let headers = request.headers_mut();
    headers.insert("User-Agent", HeaderValue::from_static(USER_AGENT));
    headers.insert(
        "Origin",
        HeaderValue::from_static("https://live.bilibili.com"),
    );
    Ok(request)
}

/// 获取弹幕服务器与鉴权 token（旧版 getConf 接口，实测游客可访问）。
/// 返回 (wss url, token)；失败时由调用方回退到默认服务器。
async fn fetch_danmu_conf(room_id: &str) -> Option<(String, String)> {
    let client = reqwest::Client::new();
    let json: Value = client
        .get(CONF_URL)
        .query(&[
            ("room_id", room_id),
            ("platform", "pc"),
            ("player", "web"),
        ])
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(
            reqwest::header::REFERER,
            format!("https://live.bilibili.com/{room_id}"),
        )
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    if json.get("code").and_then(Value::as_i64) != Some(0) {
        return None;
    }
    let data = json.get("data")?;
    let token = data.get("token")?.as_str()?.to_string();
    let host = data
        .get("host_server_list")?
        .get(0)?
        .get("host")?
        .as_str()?;
    let port = data
        .get("host_server_list")?
        .get(0)?
        .get("wss_port")
        .and_then(Value::as_u64)
        .unwrap_or(443);
    Some((format!("wss://{}:{}/sub", host, port), token))
}

/// 解析二进制帧，把其中的 JSON 消息追加到 `out`。
///
/// ver=2 为 zlib 压缩帧，解压后递归解析；ver=0/1 直接按 JSON 解析；其他版本忽略。
fn decode_frames(data: &[u8], depth: u8, out: &mut Vec<Value>) {
    if depth > MAX_DEPTH {
        return;
    }
    let mut offset = 0;
    while offset + 16 <= data.len() {
        let packet_len = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        let header_len = u16::from_be_bytes([data[offset + 4], data[offset + 5]]) as usize;
        let version = u16::from_be_bytes([data[offset + 6], data[offset + 7]]);

        // 包长非法时停止，避免越界切片或死循环
        if header_len < 16 || packet_len < header_len || offset + packet_len > data.len() {
            break;
        }

        let body = &data[offset + header_len..offset + packet_len];
        match version {
            2 => {
                let mut decoder = ZlibDecoder::new(body);
                let mut decompressed = Vec::new();
                if decoder.read_to_end(&mut decompressed).is_ok() {
                    decode_frames(&decompressed, depth + 1, out);
                }
            }
            0 | 1 => {
                if let Ok(value) = serde_json::from_slice::<Value>(body) {
                    out.push(value);
                }
            }
            _ => {} // ver=3（brotli）等版本不处理
        }
        offset += packet_len;
    }
}

/// 从消息中提取 `DANMU_MSG` 的 (用户名, 内容, 颜色)。
/// 只处理 cmd 以 `DANMU_MSG` 开头的消息（存在 `DANMU_MSG:4:0:2:2:2:0` 变体）。
fn parse_danmu(json: &Value) -> Option<(String, String, Option<String>)> {
    let cmd = json.get("cmd")?.as_str()?;
    if !cmd.starts_with("DANMU_MSG") {
        return None;
    }
    let info = json.get("info")?.as_array()?;
    // info[1] = 弹幕文本
    let text = info.get(1)?.as_str()?.to_string();
    // info[2][1] = 用户名
    let username = info
        .get(2)
        .and_then(Value::as_array)
        .and_then(|user| user.get(1))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // info[0][3] = 颜色（十进制 u32），0 视为白色
    let color = info
        .first()
        .and_then(Value::as_array)
        .and_then(|meta| meta.get(3))
        .and_then(Value::as_u64)
        .map(|c| {
            if c == 0 {
                "#FFFFFF".to_string()
            } else {
                format!("#{:06X}", c & 0xFF_FFFF)
            }
        });
    Some((username, text, color))
}

pub async fn run(app: &AppHandle, room_id: &str, tx: Sender<DanmakuMessage>) -> anyhow::Result<()> {
    log::info!("[danmaku] bilibili 开始连接房间 {}", room_id);
    // 前端传入的可能是短号（如 6），协议需要真实房间号；查询失败则沿用入参
    let real_room = fetch_real_room_id(room_id)
        .await
        .unwrap_or_else(|| room_id.to_string());
    log::info!("[danmaku] bilibili 真实房间号 {}", real_room);
    let room_number: u64 = real_room
        .parse()
        .with_context(|| format!("无效的 B站房间号: {real_room}"))?;

    let (ws_url, token) = fetch_danmu_conf(&real_room)
        .await
        .unwrap_or_else(|| (WS_URL.to_string(), String::new()));
    log::info!(
        "[danmaku] bilibili 弹幕服务器 {}（token: {}）",
        ws_url,
        if token.is_empty() { "无" } else { "有" }
    );

    let request = build_ws_request(&ws_url)?;
    let (mut ws, _response) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .context("连接 B站弹幕服务器超时")?
    .context("连接 B站弹幕服务器失败")?;

    // 认证包：protover=2 表示希望服务端下发 zlib 压缩帧；key 为 getConf 下发的鉴权 token
    let mut auth = serde_json::json!({
        "uid": 0,
        "roomid": room_number,
        "protover": 2,
        "platform": "web",
        "type": 2,
    });
    if !token.is_empty() {
        auth["key"] = Value::String(token);
    }
    ws.send(Message::Binary(
        build_packet(&serde_json::to_vec(&auth)?, OP_AUTH).into(),
    ))
    .await
    .context("发送 B站弹幕认证包失败")?;

    super::emit_status(app, PLATFORM, room_id, "connected", None);
    log::info!("[danmaku] bilibili 认证包已发送");

    let mut heartbeat = tokio::time::interval_at(
        tokio::time::Instant::now() + HEARTBEAT_INTERVAL,
        HEARTBEAT_INTERVAL,
    );

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                let packet = build_packet(b"[object Object] ", OP_HEARTBEAT);
                ws.send(Message::Binary(packet.into()))
                    .await
                    .context("发送 B站弹幕心跳失败")?;
            }
            frame = ws.next() => {
                match frame {
                    Some(Ok(Message::Binary(data))) => {
                        let mut messages = Vec::new();
                        decode_frames(&data, 0, &mut messages);
                        for value in messages {
                            let Some((username, text, color)) = parse_danmu(&value) else {
                                continue;
                            };
                            if tx
                                .send(super::message(PLATFORM, room_id, username, text, color))
                                .await
                                .is_err()
                            {
                                // 接收端已关闭，正常退出
                                return Ok(());
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        ws.send(Message::Pong(payload))
                            .await
                            .context("回复 B站弹幕 Ping 失败")?;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(anyhow!("B站弹幕连接已被服务器关闭"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::Error::from(e).context("B站弹幕连接异常"));
                    }
                    None => return Err(anyhow!("B站弹幕连接已断开")),
                }
            }
        }
    }
}
