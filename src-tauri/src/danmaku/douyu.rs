//! 斗鱼弹幕接入（TCP + STT 协议，只读）
//!
//! 流程：连接 danmuproxy → 发送 loginreq / joingroup → 每 45 秒心跳 mrkl，
//! 持续读取并解析 chatmsg 文本弹幕。

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::time::{interval, MissedTickBehavior};

use super::{emit_status, message, DanmakuMessage};

/// 弹幕主服务器
const PRIMARY_ADDR: &str = "danmuproxy.douyu.com:8601";
/// 弹幕备用服务器
const FALLBACK_ADDR: &str = "danmuproxy.douyu.com:8602";
/// 协议消息类型标识
const MSG_TYPE: u32 = 689;
/// 心跳间隔
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(45);

/// 构造一个 STT 包：length 重复两次 + type + body + 0x00 结尾
fn build_packet(body: &str) -> Vec<u8> {
    let body = body.as_bytes();
    // length = 头部中的 length 字段(4) + type(4) + body + 结尾 0x00
    let length = 9 + body.len() as u32;
    let mut packet = Vec::with_capacity(13 + body.len());
    packet.extend_from_slice(&length.to_le_bytes());
    packet.extend_from_slice(&length.to_le_bytes());
    packet.extend_from_slice(&MSG_TYPE.to_le_bytes());
    packet.extend_from_slice(body);
    packet.push(0x00);
    packet
}

/// 连接弹幕服务器，主服务器失败时回退到备用端口
async fn connect() -> Result<TcpStream> {
    match TcpStream::connect(PRIMARY_ADDR).await {
        Ok(stream) => Ok(stream),
        Err(err) => {
            log::warn!("斗鱼弹幕主服务器连接失败：{err}，尝试备用服务器");
            TcpStream::connect(FALLBACK_ADDR).await.with_context(|| {
                format!("斗鱼弹幕服务器连接失败（{PRIMARY_ADDR} 与 {FALLBACK_ADDR} 均不可用）")
            })
        }
    }
}

/// 还原 STT 转义：@S → /，@A → @
fn unescape(value: &str) -> String {
    value.replace("@S", "/").replace("@A", "@")
}

/// 把 `key@=value/` 形式的 STT 文本解码为键值对
fn decode_stt(text: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for part in text.split('/') {
        if let Some((key, value)) = part.split_once("@=") {
            fields.insert(key.to_string(), unescape(value));
        }
    }
    fields
}

/// 斗鱼颜色代码转 #RRGGBB，未识别的代码返回 None
fn color_from_code(code: Option<&str>) -> Option<String> {
    let hex = match code? {
        "0" => "#FFFFFF",
        "1" => "#FF0000",
        "2" => "#1E8C00",
        "3" => "#7AB5EB",
        "4" => "#FF7F00",
        "5" => "#9B39D4",
        "6" => "#FF5A5A",
        _ => return None,
    };
    Some(hex.to_string())
}

pub async fn run(app: &AppHandle, room_id: &str, tx: Sender<DanmakuMessage>) -> Result<()> {
    log::info!("[danmaku] douyu 开始连接房间 {}", room_id);
    if room_id.is_empty() || !room_id.bytes().all(|b| b.is_ascii_digit()) {
        bail!("斗鱼弹幕需要数字房间号");
    }

    let mut socket = connect().await?;

    // 登录包与加群包一次写入
    let mut handshake = build_packet(&format!("type@=loginreq/roomid@={room_id}/"));
    handshake.extend_from_slice(&build_packet(&format!(
        "type@=joingroup/rid@={room_id}/gid@=-9999/"
    )));
    socket
        .write_all(&handshake)
        .await
        .context("斗鱼弹幕登录包发送失败")?;

    emit_status(app, "douyu", room_id, "connected", None);
    log::info!("[danmaku] douyu 登录/加群包已发送");

    // 拆成读写两半，便于 select! 中同时持有读与写
    let (mut reader, mut writer) = socket.into_split();

    let heartbeat = build_packet("type@=mrkl/");
    let mut ticker = interval(HEARTBEAT_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // 立即完成的第一拍先消耗掉，避免刚连上就发心跳
    ticker.tick().await;

    let mut buf: Vec<u8> = Vec::with_capacity(8192);

    loop {
        buf.reserve(4096);

        tokio::select! {
            _ = ticker.tick() => {
                writer
                    .write_all(&heartbeat)
                    .await
                    .context("斗鱼弹幕心跳发送失败")?;
            }
            read = reader.read_buf(&mut buf) => {
                let n = read.context("斗鱼弹幕读取失败")?;
                if n == 0 {
                    return Err(anyhow!("斗鱼弹幕连接已关闭"));
                }

                // 按帧解析，处理粘包与半包
                let mut frames: Vec<String> = Vec::new();
                loop {
                    if buf.len() < 4 {
                        break;
                    }
                    let length =
                        u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
                    if length < 9 {
                        bail!("斗鱼弹幕帧长度非法：{length}");
                    }
                    // 整帧长度 = 4 + length
                    let total = 4 + length;
                    if buf.len() < total {
                        break;
                    }
                    // body 从 12 开始，长度 length - 9（去掉结尾 0x00）
                    let body = &buf[12..total - 1];
                    match std::str::from_utf8(body) {
                        Ok(text) => frames.push(text.to_string()),
                        Err(_) => log::warn!("斗鱼弹幕帧不是合法 UTF-8，已跳过"),
                    }
                    buf.drain(..total);
                }

                for frame in frames {
                    let fields = decode_stt(&frame);
                    if fields.get("type").map(String::as_str) != Some("chatmsg") {
                        continue;
                    }
                    let username = fields.get("nn").cloned().unwrap_or_default();
                    let text = fields.get("txt").cloned().unwrap_or_default();
                    let color = color_from_code(fields.get("col").map(String::as_str));

                    // 接收端已关闭则正常退出
                    if tx.send(message("douyu", room_id, username, text, color)).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}
