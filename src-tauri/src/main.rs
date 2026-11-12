//! StreamFlow 主入口

#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use streamflow_lib::adapter::AdapterRegistry;
use streamflow_lib::{auth, commands, db, AppState, Settings};
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

    // 应用内自更新：代理可通过 STREAMFLOW_UPDATER_PROXY 显式指定（见 commands.rs）
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(commands::PendingUpdate::default())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            greet,
            commands::search_rooms,
            commands::get_room_detail,
            commands::get_stream_urls,
            commands::get_live_status,
            commands::get_hot_rooms,
            commands::get_platform_follows,
            commands::export_auth_file,
            commands::import_auth_file,
            commands::open_external,
            commands::check_platform_connectivity,
            commands::check_platform_health,
            commands::clear_webview_cache,
            commands::check_app_update,
            commands::install_app_update,
            commands::get_app_version,
            auth::open_login_window,
            auth::complete_login,
            auth::get_auth_status,
            auth::logout,
            auth::start_qr_login,
            auth::poll_qr_login,
            db::load_favorites,
            db::save_favorites,
            db::load_history,
            db::save_history,
            db::get_setting,
            db::set_setting,
            streamflow_lib::danmaku::start_danmaku,
            streamflow_lib::danmaku::stop_danmaku,
        ])
        .setup(|app| {
            // 初始化 SQLite（app_local_data_dir/streamflow.db）
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            if let Err(e) = tauri::async_runtime::block_on(db::init(
                data_dir.join("streamflow.db").to_string_lossy().as_ref(),
            )) {
                log::error!("SQLite 初始化失败: {}", e);
            }

            // 启动时加载已保存的平台登录会话并注入适配器
            let state = app.state::<AppState>();
            auth::load_into_adapters(app.handle(), &state);

            // 诊断开关：STREAMFLOW_DEBUG_LOGIN=huya|douyu|bilibili 时启动后自动打开
            // 对应平台的登录窗口，用于排查 WebView2 白屏；正常启动不触发。
            if let Ok(platform) = std::env::var("STREAMFLOW_DEBUG_LOGIN") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = auth::open_login_window(handle, platform).await {
                        log::warn!("调试登录窗口打开失败: {}", e);
                    }
                });
            }

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
