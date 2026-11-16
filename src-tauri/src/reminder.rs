//! 开播提醒：轮询收藏主播直播状态，sleeping→live 变化时发桌面通知。
//!
//! 设计：
//! - 开关存 settings（`reminder_enabled`，默认关），轮询任务常驻、每轮检查开关
//! - 间隔 5 分钟；收藏按平台过滤（hidden_follow_platforms）后并发 3 路查状态
//! - 上轮已知状态缓存在内存；**重启后首轮只建立基线、不通知**（防止重启后全量轰炸）
//! - 通知：主播名 + 直播标题（点击行为暂用系统默认，后续可接深链）

use std::collections::HashMap;
use std::sync::Mutex;

use futures::stream::{self, StreamExt};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::LiveStatus;

/// 上轮已知状态缓存（platform|room_id → live/sleeping）
static KNOWN: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

const POLL_INTERVAL_SECS: u64 = 300; // 5 分钟
const CONCURRENCY: usize = 3; // 查询并发（防平台风控）

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // 首轮延迟 30s，等启动流程（登录态加载）完成
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            if let Err(e) = tick(&app).await {
                log::warn!("[reminder] 轮询失败: {}", e);
            }
            tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
        }
    });
}

async fn tick(app: &AppHandle) -> anyhow::Result<()> {
    // 开关：默认关
    let enabled = crate::db::get_setting("reminder_enabled".into())
        .await
        .unwrap_or(None)
        .unwrap_or_default()
        == "true";
    if !enabled {
        return Ok(());
    }
    let favorites = crate::db::load_favorites().await.unwrap_or_default();
    if favorites.is_empty() {
        return Ok(());
    }
    // 隐藏平台过滤（与关注页可见性开关一致）
    let hidden: Vec<String> = crate::db::get_setting("hidden_follow_platforms".into())
        .await
        .unwrap_or(None)
        .unwrap_or_default()
        .split(',')
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .collect();

    let state = app.state::<crate::AppState>();
    let registry = &state.adapters;
    let targets: Vec<_> = favorites
        .into_iter()
        .filter(|f| !hidden.iter().any(|h| h == &f.platform))
        .collect();
    if targets.is_empty() {
        return Ok(());
    }

    // 先取状态快照（锁内不做任何 await）
    let (first_run, mut snapshot): (bool, HashMap<String, String>) = {
        let mut guard = KNOWN.lock().unwrap();
        let known = guard.get_or_insert_with(HashMap::new);
        let first = known.is_empty();
        (first, known.clone())
    };

    // 并发 3 路查状态；单房间失败视为 sleeping（下轮再查）
    let results: Vec<(crate::db::FavoriteRecord, bool)> = stream::iter(targets)
        .map(|fav| async move {
            let now_live = match registry.get(&fav.platform) {
                Some(a) => a
                    .live_status(&fav.id)
                    .await
                    .map(|s| matches!(s, LiveStatus::Live))
                    .unwrap_or(false),
                None => false,
            };
            (fav, now_live)
        })
        .buffer_unordered(CONCURRENCY)
        .collect()
        .await;

    let mut notified = 0;
    for (fav, now_live) in results {
        let key = format!("{}|{}", fav.platform, fav.id);
        let prev = snapshot.get(&key).cloned();
        // sleeping→live 且非首轮 → 通知
        if !first_run && now_live && prev.as_deref() != Some("live") {
            app.notification()
                .builder()
                .title(format!("🔴 {} 开播了", fav.username))
                .body(&fav.title)
                .show();
            notified += 1;
            log::info!("[reminder] {}（{}）开播通知已发送", fav.username, fav.platform);
        }
        snapshot.insert(
            key,
            if now_live {
                "live".to_string()
            } else {
                "sleeping".to_string()
            },
        );
    }
    // 回写状态缓存（锁内无 await）
    *KNOWN.lock().unwrap().get_or_insert_with(HashMap::new) = snapshot;
    if notified > 0 {
        log::info!("[reminder] 本轮发送 {} 条开播通知", notified);
    }
    Ok(())
}
