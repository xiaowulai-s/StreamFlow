//! 快手弹幕 WebSocket 协议解析

#[derive(Debug, Clone)]
pub struct DanmakuMessage {
    pub color: String,
    pub time: f64,
    pub message: String,
}

pub fn get_danmaku_url(_room_id: &str) -> Option<String> {
    None
}

pub fn parse_danmaku_message(_message: &str) -> Option<DanmakuMessage> {
    None
}
