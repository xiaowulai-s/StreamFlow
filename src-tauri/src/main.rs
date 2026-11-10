//! StreamFlow 主入口

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use streamflow_lib::adapter::AdapterRegistry;
use streamflow_lib::{commands, AppState, Settings};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}

/// 构建平台适配器注册表
fn build_registry() -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();
    registry.register(streamflow_lib::platforms::huya::HuyaAdapter::new());
    registry.register(streamflow_lib::platforms::douyu::DouyuAdapter::new());
    registry.register(streamflow_lib::platforms::bilibili::BilibiliAdapter::new());
    registry.register(streamflow_lib::platforms::douyin::DouyinAdapter::new());
    registry.register(streamflow_lib::platforms::kuaishou::KuaishouAdapter::new());
    registry
}

/// Windows: 兜底显式指定 WebView2 运行时路径。
/// 部分环境下 WebView2Loader 的注册表运行时发现会异常（报
/// 0x8000FFFF 灾难性故障 / 0x800700AA 资源占用中），显式指定可彻底绕过。
/// 已设置 WEBVIEW2_BROWSER_EXECUTABLE_FOLDER 时尊重用户配置，不做覆盖。
#[cfg(target_os = "windows")]
fn ensure_webview2_runtime_folder() {
    if std::env::var_os("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER").is_some() {
        return;
    }
    let base = std::path::Path::new(r"C:\Program Files (x86)\Microsoft\EdgeWebView\Application");
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut best: Option<(Vec<u32>, std::path::PathBuf)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let ver: Vec<u32> = name
            .split('.')
            .map(|p| p.parse::<u32>())
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        if ver.is_empty() {
            continue;
        }
        let dir = entry.path();
        if !dir.join("msedgewebview2.exe").exists() {
            continue;
        }
        if best.as_ref().map_or(true, |(v, _)| ver > *v) {
            best = Some((ver, dir));
        }
    }
    if let Some((_, dir)) = best {
        log::info!("WebView2 运行时: {}", dir.display());
        std::env::set_var("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", dir);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    ensure_webview2_runtime_folder();

    streamflow_lib::init()?;

    let state = AppState {
        rooms: std::sync::Arc::new(tokio::sync::RwLock::new(vec![])),
        favorites: std::sync::Arc::new(tokio::sync::RwLock::new(vec![])),
        settings: std::sync::Arc::new(tokio::sync::RwLock::new(Settings::default())),
        adapters: build_registry(),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::search_rooms,
            commands::get_room_detail,
            commands::get_stream_urls,
            commands::get_live_status,
        ])
        .run(tauri::generate_context!())?;

    Ok(())
}

fn main() {
    run().expect("error while running tauri application");
}
