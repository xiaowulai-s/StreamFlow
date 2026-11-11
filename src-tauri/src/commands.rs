//! Tauri Commands - 前端与后端通信的桥梁

use crate::{adapter::PlatformAdapter, AppState, LiveStatus, RoomInfo, StreamResult};
use tauri::State;

/// 按平台名获取适配器
fn get_adapter<'a>(
    state: &'a State<AppState>,
    platform: &str,
) -> Result<&'a Box<dyn PlatformAdapter>, String> {
    state
        .adapters
        .get(platform)
        .ok_or_else(|| format!("未知平台: {}", platform))
}

#[tauri::command]
pub async fn search_rooms(
    state: State<'_, AppState>,
    platform: String,
    keyword: String,
) -> Result<Vec<RoomInfo>, String> {
    let adapter = get_adapter(&state, &platform)?;
    adapter.search(&keyword).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_room_detail(
    state: State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<RoomInfo, String> {
    let adapter = get_adapter(&state, &platform)?;
    adapter
        .room_detail(&room_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_stream_urls(
    state: State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<StreamResult, String> {
    let adapter = get_adapter(&state, &platform)?;
    adapter
        .stream_urls(&room_id)
        .await
        .map_err(|e| format!("{:?}", e))
}

#[tauri::command]
pub async fn get_live_status(
    state: State<'_, AppState>,
    platform: String,
    room_id: String,
) -> Result<LiveStatus, String> {
    let adapter = get_adapter(&state, &platform)?;
    adapter
        .live_status(&room_id)
        .await
        .map_err(|e| e.to_string())
}

/// 聚合所有已接入平台的热门/推荐直播间（仅已实现该能力的平台返回数据）
#[tauri::command]
pub async fn get_hot_rooms(state: State<'_, AppState>) -> Result<Vec<RoomInfo>, String> {
    let mut all: Vec<RoomInfo> = Vec::new();
    for adapter in state.adapters.all() {
        match adapter.hot_rooms().await {
            Ok(rooms) => all.extend(rooms),
            Err(e) => log::warn!("{} 热门列表获取失败: {}", adapter.name(), e),
        }
    }
    Ok(all)
}
