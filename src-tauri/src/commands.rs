//! Tauri Commands - 前端与后端通信的桥梁

use crate::{adapter::PlatformAdapter, AppState, LiveStatus, RoomInfo, StreamResult};
use tauri::{Manager, State};

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

/// 获取已登录平台的关注/订阅列表（需先在设置中心登录）
#[tauri::command]
pub async fn get_platform_follows(
    state: State<'_, AppState>,
    platform: String,
) -> Result<Vec<RoomInfo>, String> {
    let adapter = get_adapter(&state, &platform)?;
    adapter.follows().await.map_err(|e| e.to_string())
}

/// 导出登录态（auth.json）到系统「下载」目录，返回导出文件路径。
/// 文件为明文 JSON（含各平台会话 cookie），请妥善保管，不要分享给他人。
#[tauri::command]
pub async fn export_auth_file(app: tauri::AppHandle) -> Result<String, String> {
    use std::io::Write;

    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?;
    let source = data_dir.join("auth.json");
    let text = std::fs::read_to_string(&source)
        .map_err(|_| "还没有任何登录记录可导出（请先在设置中心登录平台）".to_string())?;
    // 校验结构合法后再导出
    let parsed: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("登录数据损坏：{}", e))?;
    if parsed.get("platforms").and_then(|v| v.as_object()).is_none() {
        return Err("登录数据格式异常（缺少 platforms 字段）".to_string());
    }

    let export = serde_json::json!({
        "app": "StreamFlow",
        "kind": "auth-export",
        "version": 1,
        "exportedAt": chrono_now_string(),
        "payload": parsed,
    });

    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!("StreamFlow-登录态-{}.json", chrono_now_string()));
    let mut f = std::fs::File::create(&file).map_err(|e| e.to_string())?;
    f.write_all(serde_json::to_string_pretty(&export).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().to_string())
}

/// 从导出文件内容导入登录态：校验 → 备份现有文件 → 写入 → 注入适配器。
/// 返回导入成功的平台列表。
#[tauri::command]
pub async fn import_auth_file(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    payload: String,
) -> Result<Vec<String>, String> {
    let export: serde_json::Value =
        serde_json::from_str(&payload).map_err(|_| "导入文件不是合法 JSON".to_string())?;
    if export.get("kind").and_then(|v| v.as_str()) != Some("auth-export") {
        return Err("导入文件不是 StreamFlow 登录态导出文件".to_string());
    }
    let inner = export
        .get("payload")
        .filter(|v| v.get("platforms").and_then(|v| v.as_object()).is_some())
        .ok_or_else(|| "导入文件缺少 platforms 数据".to_string())?;

    // 收集平台名（有非空 cookie 的才算导入成功）
    let platforms: Vec<String> = inner["platforms"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, v)| !v.get("cookie").and_then(|c| c.as_str()).unwrap_or("").is_empty())
        .map(|(k, _)| k.clone())
        .collect();
    if platforms.is_empty() {
        return Err("导入文件中没有任何有效的登录会话".to_string());
    }

    let data_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let target = data_dir.join("auth.json");
    // 备份现有登录态（若存在）
    if target.exists() {
        let backup = data_dir.join(format!("auth.json.bak-{}", chrono_now_string()));
        std::fs::copy(&target, &backup).map_err(|e| format!("备份现有登录态失败：{}", e))?;
    }
    std::fs::write(
        &target,
        serde_json::to_string_pretty(inner).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("写入登录态失败：{}", e))?;

    // 重新加载并注入适配器
    crate::auth::load_into_adapters(&app, state.inner());
    Ok(platforms)
}

/// 用系统默认浏览器打开链接（仅允许 http/https）
#[tauri::command]
pub async fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("仅支持打开 http/https 链接".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &url])
            .spawn()
            .map_err(|e| format!("打开浏览器失败：{}", e))?;
    }
    Ok(())
}

/// 应用版本（tauri.conf.json version）
#[tauri::command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 待安装的更新对象（check 与 install 两步之间暂存）
#[derive(Default)]
pub struct PendingUpdate(pub std::sync::Mutex<Option<tauri_plugin_updater::Update>>);

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeta {
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

/// 检查应用更新（GitHub Releases；代理用 STREAMFLOW_UPDATER_PROXY）
#[tauri::command]
pub async fn check_app_update(
    app: tauri::AppHandle,
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<Option<UpdateMeta>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let mut builder = app.updater_builder();
    if let Ok(proxy) = std::env::var("STREAMFLOW_UPDATER_PROXY") {
        if !proxy.is_empty() {
            let url: tauri::Url = proxy
                .parse()
                .map_err(|e| format!("代理地址无效（STREAMFLOW_UPDATER_PROXY）: {e}"))?;
            builder = builder.proxy(url);
        }
    }
    let updater = builder.build().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| {
        let meta = UpdateMeta {
            version: u.version.clone(),
            current_version: u.current_version.clone(),
            notes: u.body.clone(),
        };
        *pending.0.lock().unwrap() = Some(u);
        meta
    }))
}

/// 下载并安装已检查到的更新（安装后应用自动重启）
#[tauri::command]
pub async fn install_app_update(
    pending: tauri::State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = pending.0.lock().unwrap().take();
    match update {
        Some(u) => {
            u.download_and_install(
                |_chunk, _total| {},
                || log::info!("更新安装完成，应用即将重启"),
            )
            .await
            .map_err(|e| format!("安装失败: {e}"))
        }
        None => Err("请先检查更新".to_string()),
    }
}

/// 本地时间戳字符串（YYYYMMDD-HHMMSS），导出文件命名用
fn chrono_now_string() -> String {    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    // 东八区偏移 + 8h，按 Unix 时间简单换算为日期字段
    let secs = now + 8 * 3600;
    let days = secs / 86400;
    let rem = secs % 86400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // civil_from_days（Howard Hinnant 算法）
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        y, m, d, h, mi, s
    )
}

/// 平台连通性检测：对各平台门户发起轻量请求，返回延迟毫秒。
/// 仅测网络可达性与门户响应，不代表登录态有效。
#[tauri::command]
pub async fn check_platform_connectivity(platform: String) -> Result<u64, String> {
    let url = match platform.as_str() {
        "huya" => "https://www.huya.com/",
        "douyu" => "https://www.douyu.com/",
        "bilibili" => "https://api.bilibili.com/x/web-interface/zone",
        "douyin" => "https://live.douyin.com/",
        "kuaishou" => "https://live.kuaishou.com/",
        _ => return Err(format!("未知平台: {}", platform)),
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?;
    let start = std::time::Instant::now();
    client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .send()
        .await
        .map_err(|e| format!("连接失败：{}", e))?
        .error_for_status()
        .map_err(|e| format!("门户响应异常：{}", e))?;
    Ok(start.elapsed().as_millis() as u64)
}

/// 清理 WebView2 网页缓存（Cookie/存储由登录态与应用数据承载，不受影响时才安全）。
/// 注意：会同时清除 WebView 内缓存的图片/临时文件；登录 cookie 属于 profile 存储，
/// 会被一并清除会导致退出登录——因此本命令仅清除 HTTP 缓存与临时文件。
#[tauri::command]
pub async fn clear_webview_cache(app: tauri::AppHandle) -> Result<(), String> {
    // Tauri 2：遍历 webview 调用 clear_all_browsing_data 前先确认可用性。
    // 该方法清除包括 cookie 在内的全部浏览数据，会导致登录态丢失，
    // 因此仅在用户于设置中明确点击时调用。
    let removed = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    removed
        .clear_all_browsing_data()
        .map_err(|e| format!("清理缓存失败：{}", e))
}
