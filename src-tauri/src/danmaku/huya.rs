//! 虎牙弹幕接入（只读）
//!
//! 虎牙弹幕走 TARS 二进制协议 over WebSocket：
//! 1. 先请求直播间页面，提取主播 uid 与频道号（top_sid / sub_sid）；
//! 2. 连接 `wss://cdnws.api.huya.com/`，发送 `WSUserInfo` 注册包（WebSocketCommand.iCmdType=1）；
//! 3. 每 60 秒发送一次协议固定心跳，维持连接；
//! 4. 下行 `WebSocketCommand`（iCmdType=7）里内层消息类型 1400 即弹幕（MessageNotice）。

use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use futures_util::{SinkExt, StreamExt};
use regex::Regex;
use tauri::AppHandle;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use super::DanmakuMessage;

/// 虎牙弹幕 WebSocket 地址
const WSS_URL: &str = "wss://cdnws.api.huya.com/";
/// 与网页端一致的 UA，缺少时可能拿不到房间信息或注册被拒
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
/// 心跳间隔（协议要求 60 秒）
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(60);
/// WebSocketCommand.iCmdType：注册
const CMD_REGISTER: i32 = 1;
/// WebSocketCommand.iCmdType：消息推送
const CMD_MSG_PUSH: i32 = 7;
/// 弹幕的消息类型（内层 tag1）
const MSG_TYPE_DANMAKU: i64 = 1400;

/// 协议固定心跳包（TARS 编码后的常量，属于协议本身）
const HEARTBEAT: &[u8] = &[
    0x00, 0x03, 0x1d, 0x00, 0x00, 0x69, 0x00, 0x00, 0x00, 0x69, 0x10, 0x03, 0x2c, 0x3c, 0x4c, 0x56,
    0x08, 0x6f, 0x6e, 0x6c, 0x69, 0x6e, 0x65, 0x75, 0x69, 0x66, 0x0f, 0x4f, 0x6e, 0x55, 0x73, 0x65,
    0x72, 0x48, 0x65, 0x61, 0x72, 0x74, 0x42, 0x65, 0x61, 0x74, 0x7d, 0x00, 0x00, 0x3c, 0x08, 0x00,
    0x01, 0x06, 0x04, 0x74, 0x52, 0x65, 0x71, 0x1d, 0x00, 0x00, 0x2f, 0x0a, 0x0a, 0x0c, 0x16, 0x00,
    0x26, 0x00, 0x36, 0x07, 0x61, 0x64, 0x72, 0x5f, 0x77, 0x61, 0x70, 0x46, 0x00, 0x0b, 0x12, 0x03,
    0xae, 0xf0, 0x0f, 0x22, 0x03, 0xae, 0xf0, 0x0f, 0x3c, 0x42, 0x6d, 0x52, 0x02, 0x60, 0x5c, 0x60,
    0x01, 0x7c, 0x82, 0x00, 0x0b, 0xb0, 0x1f, 0x9c, 0xac, 0x0b, 0x8c, 0x98, 0x0c, 0xa8, 0x0c, 0x20,
];

/// 连接虎牙直播间并持续把弹幕转发到 `tx`。
///
/// 连接建立并注册成功后会推送 `connected` 状态；连接结束或出错时返回 `Err`，
/// 由上层统一推送 `closed` / `error`。
pub async fn run(app: &AppHandle, room_id: &str, tx: Sender<DanmakuMessage>) -> anyhow::Result<()> {
    log::info!("[danmaku] huya 开始连接房间 {}", room_id);
    let (uid, top_sid, sub_sid) = fetch_room_ids(room_id).await?;
    log::info!(
        "[danmaku] huya 房间信息 uid={} top_sid={} sub_sid={}",
        uid,
        top_sid,
        sub_sid
    );

    // 自定义请求头需要先构造 http 请求，再交给 connect_async
    let mut request = WSS_URL
        .into_client_request()
        .context("构建虎牙弹幕 WebSocket 请求失败")?;
    let user_agent = USER_AGENT
        .parse()
        .map_err(|err| anyhow!("非法的 User-Agent: {err}"))?;
    let _ = request.headers_mut().insert("User-Agent", user_agent);

    let (ws, _) = tokio::time::timeout(
        Duration::from_secs(10),
        tokio_tungstenite::connect_async(request),
    )
    .await
    .context("连接虎牙弹幕服务器超时")?
    .context("连接虎牙弹幕服务器失败")?;
    log::info!("[danmaku] huya WebSocket 已连接");
    // 拆分成发送/接收两半，便于在 select! 中同时处理心跳与下行消息
    let (mut sink, mut stream) = ws.split();

    let packet = build_registration_packet(uid, top_sid, sub_sid);
    sink.send(Message::Binary(packet))
        .await
        .context("发送虎牙弹幕注册包失败")?;
    log::info!("[danmaku] huya 注册包已发送");

    super::emit_status(app, "huya", room_id, "connected", None);

    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    // interval 的第一次 tick 会立即返回，先消费掉，避免注册后马上重复发心跳
    heartbeat.tick().await;

    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                sink.send(Message::Binary(HEARTBEAT.to_vec()))
                    .await
                    .context("发送虎牙心跳失败")?;
            }
            incoming = stream.next() => {
                let Some(incoming) = incoming else {
                    bail!("虎牙弹幕连接已结束");
                };
                let incoming = incoming.context("读取虎牙弹幕数据失败")?;
                match incoming {
                    Message::Ping(payload) => {
                        sink.send(Message::Pong(payload))
                            .await
                            .context("回应虎牙 Ping 失败")?;
                    }
                    Message::Close(_) => bail!("虎牙弹幕连接被服务器关闭"),
                    other => {
                        let data = other.into_data();
                        if data.is_empty() {
                            continue;
                        }
                        if let Some((username, text)) = parse_danmaku(&data) {
                            let message = super::message("huya", room_id, username, text, None);
                            if tx.send(message).await.is_err() {
                                // 接收端已释放，正常退出
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 请求直播间页面，提取 uid 与频道号。
///
/// 页面里的 `lChannelId` / `lSubChannelId` 可能缺失，此时退化为使用 uid。
async fn fetch_room_ids(room_id: &str) -> anyhow::Result<(i64, i64, i64)> {
    let url = format!("https://www.huya.com/{room_id}");
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(10))
        .build()
        .context("创建 HTTP 客户端失败")?;
    let html = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("请求虎牙直播间页面失败: {url}"))?
        .error_for_status()
        .with_context(|| format!("虎牙直播间页面返回异常状态: {url}"))?
        .text()
        .await
        .context("读取虎牙直播间页面失败")?;

    let Some(uid) = extract_number(&html, r#""lPresenterUid":(\d+)"#)? else {
        bail!("未能从虎牙直播间页面解析到主播 uid: {room_id}");
    };
    let top_sid = extract_number(&html, r#""lChannelId":(\d+)"#)?.unwrap_or(uid);
    let sub_sid = extract_number(&html, r#""lSubChannelId":(\d+)"#)?.unwrap_or(uid);
    Ok((uid, top_sid, sub_sid))
}

/// 按正则取第一个捕获组的数字（取不到返回 None）。
fn extract_number(html: &str, pattern: &str) -> anyhow::Result<Option<i64>> {
    let re = Regex::new(pattern).with_context(|| format!("非法正则表达式: {pattern}"))?;
    Ok(re
        .captures(html)
        .and_then(|caps| caps.get(1))
        .and_then(|matched| matched.as_str().parse().ok()))
}

/// 构造注册包：WSUserInfo 结构包进 WebSocketCommand（iCmdType=1，vData=WSUserInfo）。
fn build_registration_packet(uid: i64, top_sid: i64, sub_sid: i64) -> Vec<u8> {
    let mut user_info = TarsWriter::new();
    user_info.write_int64(0, uid); // lUid
    user_info.write_bool(1, false); // bAnonymous
    user_info.write_string(2, ""); // sGuid
    user_info.write_string(3, ""); // sToken
    user_info.write_int64(4, top_sid); // lTid
    user_info.write_int64(5, sub_sid); // lSid
    user_info.write_int64(6, uid); // lGroupId
    user_info.write_int64(7, 3); // lGroupType
    let user_info = user_info.into_vec();

    let mut command = TarsWriter::new();
    command.write_int32(0, CMD_REGISTER);
    command.write_bytes(1, &user_info);
    command.into_vec()
}

/// 解析一帧下行数据，命中弹幕时返回 (昵称, 内容)。
fn parse_danmaku(frame: &[u8]) -> Option<(String, String)> {
    let mut command = TarsReader::new(frame);
    if command.read_int32(0)? != CMD_MSG_PUSH {
        return None;
    }
    let payload = command.read_bytes(1)?;

    let mut push = TarsReader::new(&payload);
    if push.read_int64(1)? != MSG_TYPE_DANMAKU {
        return None;
    }
    let notice_data = push.read_bytes(2)?;

    // MessageNotice：tag0 发送者结构（tag2 昵称），tag3 弹幕内容
    let mut notice = TarsReader::new(&notice_data);
    let username = notice
        .read_struct(0, |sender| sender.read_string(2).unwrap_or_default())
        .unwrap_or_default();
    let text = notice.read_string(3).unwrap_or_default();
    if text.is_empty() {
        return None;
    }
    Some((username, text))
}

// ---------------------------------------------------------------------------
// 最小 TARS 编解码（只覆盖虎牙弹幕用到的子集）
// ---------------------------------------------------------------------------

const TARS_INT8: u8 = 0;
const TARS_INT16: u8 = 1;
const TARS_INT32: u8 = 2;
const TARS_INT64: u8 = 3;
const TARS_FLOAT: u8 = 4;
const TARS_DOUBLE: u8 = 5;
const TARS_STRING1: u8 = 6;
const TARS_STRING4: u8 = 7;
const TARS_MAP: u8 = 8;
const TARS_LIST: u8 = 9;
const TARS_STRUCT_BEGIN: u8 = 10;
const TARS_STRUCT_END: u8 = 11;
const TARS_ZERO: u8 = 12;
const TARS_BYTES: u8 = 13;

/// 跳过嵌套字段时的最大递归深度，防止畸形数据打爆线程栈。
const TARS_MAX_DEPTH: usize = 64;

/// TARS 输出流
struct TarsWriter {
    buffer: Vec<u8>,
}

impl TarsWriter {
    fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(64),
        }
    }

    fn into_vec(self) -> Vec<u8> {
        self.buffer
    }

    /// 写数据头：低 4 位是类型，高 4 位是 tag（tag>=15 时用 0xF0 + 一字节 tag）
    fn write_head(&mut self, tag: u8, type_id: u8) {
        if tag < 15 {
            self.buffer.push((tag << 4) | type_id);
        } else {
            self.buffer.push(0xF0 | type_id);
            self.buffer.push(tag);
        }
    }

    /// 按值域选择最紧凑的整型编码（TARS 的常规做法）
    fn write_number(&mut self, tag: u8, value: i64) {
        if value == 0 {
            self.write_head(tag, TARS_ZERO);
        } else if let Ok(value) = i8::try_from(value) {
            self.write_head(tag, TARS_INT8);
            self.buffer.push(value as u8);
        } else if let Ok(value) = i16::try_from(value) {
            self.write_head(tag, TARS_INT16);
            self.buffer.extend_from_slice(&value.to_be_bytes());
        } else if let Ok(value) = i32::try_from(value) {
            self.write_head(tag, TARS_INT32);
            self.buffer.extend_from_slice(&value.to_be_bytes());
        } else {
            self.write_head(tag, TARS_INT64);
            self.buffer.extend_from_slice(&value.to_be_bytes());
        }
    }

    fn write_int32(&mut self, tag: u8, value: i32) {
        self.write_number(tag, i64::from(value));
    }

    fn write_int64(&mut self, tag: u8, value: i64) {
        self.write_number(tag, value);
    }

    fn write_bool(&mut self, tag: u8, value: bool) {
        self.write_number(tag, if value { 1 } else { 0 });
    }

    fn write_string(&mut self, tag: u8, value: &str) {
        let bytes = value.as_bytes();
        if let Ok(len) = u8::try_from(bytes.len()) {
            self.write_head(tag, TARS_STRING1);
            self.buffer.push(len);
        } else {
            self.write_head(tag, TARS_STRING4);
            let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
            self.buffer.extend_from_slice(&len.to_be_bytes());
        }
        self.buffer.extend_from_slice(bytes);
    }

    fn write_bytes(&mut self, tag: u8, value: &[u8]) {
        self.write_head(tag, TARS_BYTES);
        self.write_head(0, TARS_INT8); // 简单列表的元素类型
        self.write_int32(0, i32::try_from(value.len()).unwrap_or(i32::MAX));
        self.buffer.extend_from_slice(value);
    }

    /// 结构体开始标记（当前注册包按协议直接用裸字段，保留给需要显式结构的场景）
    #[allow(dead_code)]
    fn write_struct_begin(&mut self, tag: u8) {
        self.write_head(tag, TARS_STRUCT_BEGIN);
    }

    /// 结构体结束标记
    #[allow(dead_code)]
    fn write_struct_end(&mut self) {
        self.write_head(0, TARS_STRUCT_END);
    }
}

/// TARS 输入流
struct TarsReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> TarsReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// 读取并前移数据头，返回 (tag, 类型)
    fn read_head(&mut self) -> Option<(u8, u8)> {
        let first = *self.data.get(self.pos)?;
        self.pos += 1;
        let type_id = first & 0x0F;
        let mut tag = first >> 4;
        if tag == 0x0F {
            tag = *self.data.get(self.pos)?;
            self.pos += 1;
        }
        Some((tag, type_id))
    }

    /// 预读数据头，不移动游标
    fn peek_head(&self) -> Option<(u8, u8)> {
        let first = *self.data.get(self.pos)?;
        let type_id = first & 0x0F;
        let mut tag = first >> 4;
        if tag == 0x0F {
            tag = *self.data.get(self.pos + 1)?;
        }
        Some((tag, type_id))
    }

    /// 取走 `len` 字节，越界返回 None
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    /// 跳到指定 tag 的字段上（不消费该字段的头）。
    ///
    /// 遇到结构结束或比目标更大的 tag 时返回 false，避免越界误读。
    fn skip_to_tag(&mut self, target: u8) -> bool {
        while let Some((tag, type_id)) = self.peek_head() {
            if type_id == TARS_STRUCT_END || tag > target {
                return false;
            }
            if tag == target {
                return true;
            }
            self.read_head();
            if !self.skip_value(type_id, 0) {
                return false;
            }
        }
        false
    }

    /// 读取容器长度（长度本身也是一个带头部的字段）
    fn read_length(&mut self) -> Option<usize> {
        let (_, type_id) = self.read_head()?;
        let value = self.read_integer(type_id)?;
        usize::try_from(value).ok()
    }

    /// 读取整型数值（调用前数据头已被消费）
    fn read_integer(&mut self, type_id: u8) -> Option<i64> {
        match type_id {
            TARS_ZERO => Some(0),
            TARS_INT8 => {
                let value = *self.take(1)?.first()?;
                Some(i64::from(value as i8))
            }
            TARS_INT16 => {
                let bytes = self.take(2)?;
                Some(i64::from(i16::from_be_bytes([bytes[0], bytes[1]])))
            }
            TARS_INT32 => {
                let bytes = self.take(4)?;
                Some(i64::from(i32::from_be_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3],
                ])))
            }
            TARS_INT64 => {
                let bytes = self.take(8)?;
                Some(i64::from_be_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]))
            }
            _ => None,
        }
    }

    /// 跳过一个已读取头部的字段（含 Struct/List/Map 的递归跳过）
    fn skip_value(&mut self, type_id: u8, depth: usize) -> bool {
        if depth > TARS_MAX_DEPTH {
            self.pos = self.data.len();
            return false;
        }
        match type_id {
            TARS_INT8 => self.take(1).is_some(),
            TARS_INT16 => self.take(2).is_some(),
            TARS_INT32 | TARS_FLOAT => self.take(4).is_some(),
            TARS_INT64 | TARS_DOUBLE => self.take(8).is_some(),
            TARS_ZERO => true,
            TARS_STRING1 => match self.take(1) {
                Some(len) => self.take(usize::from(len[0])).is_some(),
                None => false,
            },
            TARS_STRING4 => match self.take(4) {
                Some(bytes) => {
                    let len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                    self.take(len).is_some()
                }
                None => false,
            },
            TARS_BYTES => {
                // bytes：元素类型头 + 长度字段 + 原始数据
                if self.read_head().is_none() {
                    return false;
                }
                match self.read_length() {
                    Some(len) => self.take(len).is_some(),
                    None => false,
                }
            }
            TARS_LIST => self.skip_container(depth, 1),
            TARS_MAP => self.skip_container(depth, 2),
            TARS_STRUCT_BEGIN => loop {
                let Some((_, inner)) = self.read_head() else {
                    return false;
                };
                if inner == TARS_STRUCT_END {
                    return true;
                }
                if !self.skip_value(inner, depth + 1) {
                    return false;
                }
            },
            TARS_STRUCT_END => true,
            _ => {
                self.pos = self.data.len();
                false
            }
        }
    }

    /// 跳过 List / Map 的每个元素；`factor` 为 Map 时的键值对数量系数
    fn skip_container(&mut self, depth: usize, factor: usize) -> bool {
        let Some(len) = self.read_length() else {
            return false;
        };
        let count = len.saturating_mul(factor);
        for _ in 0..count {
            let Some((_, item_type)) = self.read_head() else {
                return false;
            };
            if !self.skip_value(item_type, depth + 1) {
                return false;
            }
        }
        true
    }

    /// 读取指定 tag 的 int32
    fn read_int32(&mut self, tag: u8) -> Option<i32> {
        if !self.skip_to_tag(tag) {
            return None;
        }
        let (_, type_id) = self.read_head()?;
        let value = self.read_integer(type_id)?;
        i32::try_from(value).ok()
    }

    /// 读取指定 tag 的 int64
    fn read_int64(&mut self, tag: u8) -> Option<i64> {
        if !self.skip_to_tag(tag) {
            return None;
        }
        let (_, type_id) = self.read_head()?;
        self.read_integer(type_id)
    }

    /// 读取指定 tag 的字符串
    fn read_string(&mut self, tag: u8) -> Option<String> {
        if !self.skip_to_tag(tag) {
            return None;
        }
        let (_, type_id) = self.read_head()?;
        let len = match type_id {
            TARS_ZERO => return Some(String::new()),
            TARS_STRING1 => {
                let len = *self.take(1)?.first()?;
                usize::from(len)
            }
            TARS_STRING4 => {
                let bytes = self.take(4)?;
                u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize
            }
            _ => {
                self.skip_value(type_id, 0);
                return None;
            }
        };
        let bytes = self.take(len)?;
        Some(String::from_utf8_lossy(bytes).into_owned())
    }

    /// 读取指定 tag 的 bytes
    fn read_bytes(&mut self, tag: u8) -> Option<Vec<u8>> {
        if !self.skip_to_tag(tag) {
            return None;
        }
        let (_, type_id) = self.read_head()?;
        if type_id != TARS_BYTES {
            self.skip_value(type_id, 0);
            return None;
        }
        self.read_head()?; // 元素类型头
        let len = self.read_length()?;
        Some(self.take(len)?.to_vec())
    }

    /// 读取指定 tag 的结构体，闭包负责读内部字段；读完自动跳到结构结束
    fn read_struct<T>(&mut self, tag: u8, f: impl FnOnce(&mut Self) -> T) -> Option<T> {
        if !self.skip_to_tag(tag) {
            return None;
        }
        let (_, type_id) = self.read_head()?;
        if type_id != TARS_STRUCT_BEGIN {
            self.skip_value(type_id, 0);
            return None;
        }
        let value = f(self);
        while let Some((_, inner)) = self.read_head() {
            if inner == TARS_STRUCT_END {
                break;
            }
            if !self.skip_value(inner, 0) {
                break;
            }
        }
        Some(value)
    }
}
