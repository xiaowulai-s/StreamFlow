//! 弹幕只读接入（B站 / 斗鱼 / 虎牙 / 抖音 / 快手）
//!
//! 前端调用 `start_danmaku(platform, room_id)` 后，后端建立对应平台连接，
//! 将聊天消息通过 Tauri 事件 `danmaku-message` 推给前端；
//! 连接状态通过 `danmaku-status` 推送（connecting / connected / error / closed）。

pub mod bilibili;
pub mod douyin;
pub mod douyu;
pub mod huya;
pub mod kuaishou;
pub mod proto;

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

/// 单条弹幕
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DanmakuMessage {
    pub platform: String,
    pub room_id: String,
    pub username: String,
    pub text: String,
    /// #RRGGBB（平台未提供时为空）
    pub color: Option<String>,
    pub timestamp: i64,
}

/// 弹幕连接状态
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DanmakuStatus {
    pub platform: String,
    pub room_id: String,
    /// connecting / connected / error / closed
    pub status: String,
    pub message: Option<String>,
}

pub(crate) fn emit_status(
    app: &AppHandle,
    platform: &str,
    room_id: &str,
    status: &str,
    message: Option<String>,
) {
    let _ = app.emit(
        "danmaku-status",
        DanmakuStatus {
            platform: platform.to_string(),
            room_id: room_id.to_string(),
            status: status.to_string(),
            message,
        },
    );
}

pub(crate) fn message(
    platform: &str,
    room_id: &str,
    username: String,
    text: String,
    color: Option<String>,
) -> DanmakuMessage {
    DanmakuMessage {
        platform: platform.to_string(),
        room_id: room_id.to_string(),
        username,
        text,
        color,
        timestamp: chrono::Utc::now().timestamp_millis(),
    }
}

type TaskMap = Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>;

fn tasks() -> &'static TaskMap {
    static TASKS: OnceLock<TaskMap> = OnceLock::new();
    TASKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn task_key(platform: &str, room_id: &str) -> String {
    format!("{}:{}", platform, room_id)
}

/// 开始接收某直播间的弹幕（重复调用会先停止旧连接）
#[tauri::command]
pub async fn start_danmaku(
    app: AppHandle,
    platform: String,
    room_id: String,
) -> Result<(), String> {
    let key = task_key(&platform, &room_id);
    if let Some(old) = tasks().lock().unwrap().remove(&key) {
        old.abort();
    }

    let (tx, mut rx) = mpsc::channel::<DanmakuMessage>(256);
    let forward_app = app.clone();
    let task_app = app.clone();
    let task_platform = platform.clone();
    let task_room = room_id.clone();

    let handle = tauri::async_runtime::spawn(async move {
        let forward = tauri::async_runtime::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let _ = forward_app.emit("danmaku-message", &msg);
            }
        });

        emit_status(&task_app, &task_platform, &task_room, "connecting", None);
        let result = match task_platform.as_str() {
            "bilibili" => bilibili::run(&task_app, &task_room, tx.clone()).await,
            "douyu" => douyu::run(&task_app, &task_room, tx.clone()).await,
            "huya" => huya::run(&task_app, &task_room, tx.clone()).await,
            "douyin" => douyin::run(&task_app, &task_room, tx.clone()).await,
            "kuaishou" => kuaishou::run(&task_app, &task_room, tx.clone()).await,
            _ => Err(anyhow::anyhow!("该平台暂不支持弹幕")),
        };
        match result {
            Ok(()) => emit_status(&task_app, &task_platform, &task_room, "closed", None),
            Err(e) => emit_status(
                &task_app,
                &task_platform,
                &task_room,
                "error",
                Some(e.to_string()),
            ),
        }
        drop(tx);
        forward.abort();
    });

    tasks().lock().unwrap().insert(key, handle);
    Ok(())
}

/// 停止接收弹幕
#[tauri::command]
pub fn stop_danmaku(app: AppHandle, platform: String, room_id: String) {
    let key = task_key(&platform, &room_id);
    if let Some(handle) = tasks().lock().unwrap().remove(&key) {
        handle.abort();
        emit_status(&app, &platform, &room_id, "closed", None);
    }
}
