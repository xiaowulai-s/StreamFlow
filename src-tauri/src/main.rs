//! StreamFlow 主入口

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use streamflow_lib::adapter::AdapterRegistry;
use streamflow_lib::{commands, AppState, Settings};
use tauri::Manager;

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
        .setup(|app| {
            // 窗口配置 create=false，改为手动创建并带重试：
            // 上个实例的 WebView2 浏览器进程组优雅关闭需要数秒，期间快速重启会报
            // 0x800700AA（资源占用中）。退避重试；多次失败后备份 UDF 再重建。
            let window_config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or("配置缺少窗口定义")?;

            let build_window = || {
                tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)
                    .expect("窗口配置无效")
                    .build()
            };

            let mut delay: u64 = 3;
            loop {
                match build_window() {
                    Ok(_) => break,
                    Err(e) if delay <= 12 => {
                        log::warn!("WebView2 环境创建失败（{}），{} 秒后重试", e, delay);
                        std::thread::sleep(std::time::Duration::from_secs(delay));
                        delay *= 2;
                    }
                    Err(_) => {
                        // 最终手段：上次异常退出残留损坏 UDF，备份后重建（webview 本地存储会丢失）
                        log::warn!("多次重试失败，备份 WebView2 数据目录后重建");
                        let data_dir = app.path().app_local_data_dir()?;
                        let webview_dir = data_dir.join("EBWebView");
                        if webview_dir.exists() {
                            let backup =
                                data_dir.join(format!("EBWebView.broken-{}", std::process::id()));
                            let _ = std::fs::rename(&webview_dir, &backup);
                        }
                        build_window().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                        break;
                    }
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())?;

    Ok(())
}

fn main() {
    run().expect("error while running tauri application");
}
