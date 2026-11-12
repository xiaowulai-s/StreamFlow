//! 平台登录与会话管理（阶段D：应用内嵌 WebView 登录）
//!
//! 流程：打开平台官方登录页 WebView 窗口 → 用户完成扫码/手机号登录 →
//! 通过 WebView2 CookieManager 读取该域 cookie → 持久化 auth.json → 注入适配器。
//!
//! 合规边界：仅保存用户本人登录产生的会话 cookie，不绕过任何风控/DRM；
//! cookie 明文保存在应用本地数据目录（auth.json），与浏览器本地存储同级。

use crate::{adapter::PlatformAdapter, AppState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// 平台登录配置
pub struct PlatformAuth {
    pub login_url: &'static str,
    /// GetCookies 的目标 URI（根路径即覆盖全站域 cookie）
    pub cookie_uri: &'static str,
    /// 判定已登录的关键 cookie 名（任一命中）
    pub session_cookies: &'static [&'static str],
    pub label: &'static str,
}

pub fn platform_auth(platform: &str) -> Option<PlatformAuth> {
    // 登录窗口统一打开平台主页：passport 直链各自有坑
    // （B站 passport/login 跳主站、斗鱼跳 404、虎牙把 /login 当房间号解析），
    // 主页自带的「登录」入口最稳定，登录后 cookie 均落在主域。
    match platform {
        "huya" => Some(PlatformAuth {
            login_url: "https://www.huya.com/",
            cookie_uri: "https://www.huya.com/",
            session_cookies: &["yyuid", "udb_passdata", "udb_ttl"],
            label: "虎牙",
        }),
        "douyu" => Some(PlatformAuth {
            login_url: "https://www.douyu.com/",
            cookie_uri: "https://www.douyu.com/",
            session_cookies: &["acf_uid", "acf_auth", "acf_nickname"],
            label: "斗鱼",
        }),
        "bilibili" => Some(PlatformAuth {
            login_url: "https://www.bilibili.com/",
            cookie_uri: "https://www.bilibili.com/",
            session_cookies: &["SESSDATA", "DedeUserID"],
            label: "B站",
        }),
        "kuaishou" => Some(PlatformAuth {
            // 快手弹幕 token 接口有风控，登录后会话 cookie 可正常换取
            login_url: "https://live.kuaishou.com/",
            cookie_uri: "https://live.kuaishou.com/",
            session_cookies: &["kuaishou.live.web_st", "kuaishou.server.web_st", "userId"],
            label: "快手",
        }),
        "douyin" => Some(PlatformAuth {
            // 抖音：内嵌登录后 sessionid 可解锁关键词搜索（需用户在窗口内完成扫码/验证码）
            login_url: "https://www.douyin.com/",
            cookie_uri: "https://www.douyin.com/",
            session_cookies: &["sessionid", "sessionid_ss"],
            label: "抖音",
        }),
        _ => None,
    }
}

/// 单平台会话
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformSession {
    pub cookie: String,
    pub username: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AuthStore {
    platforms: HashMap<String, PlatformSession>,
}

/// 全局会话存储（首次访问时从磁盘加载）
static AUTH_STORE: OnceLock<Mutex<AuthStore>> = OnceLock::new();

fn auth_store() -> &'static Mutex<AuthStore> {
    AUTH_STORE.get_or_init(|| Mutex::new(AuthStore::default()))
}

fn store_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("auth.json"))
}

fn load_store(app: &AppHandle) -> AuthStore {
    let Ok(path) = store_path(app) else {
        return AuthStore::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AuthStore::default(),
    }
}

fn persist(app: &AppHandle, store: &AuthStore) -> Result<(), String> {
    let path = store_path(app)?;
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// 初始化：启动时从磁盘加载并注入适配器（main.rs setup 调用）
pub fn load_into_adapters(app: &AppHandle, state: &AppState) {
    let store = load_store(app);
    let mut guard = auth_store().lock().unwrap();
    *guard = store;
    let map = guard.platforms.clone();
    drop(guard);
    for (platform, session) in map {
        if !session.cookie.is_empty() {
            state
                .adapters
                .set_platform_cookies(&platform, Some(session.cookie));
        }
    }
}

/// 读取某平台已保存的会话 cookie（弹幕等模块使用）
pub fn platform_cookie(platform: &str) -> Option<String> {
    let guard = auth_store().lock().unwrap();
    guard
        .platforms
        .get(platform)
        .map(|s| s.cookie.clone())
        .filter(|c| !c.is_empty())
}

/// 登录状态（返回前端）
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub platform: String,
    pub label: String,
    pub logged_in: bool,
    pub username: Option<String>,
    /// 是否支持应用内登录（抖音暂无登录方案，仍展示状态行）
    #[serde(default)]
    pub can_login: bool,
}

// ---------------------------------------------------------------------------
// Tauri 命令
// ---------------------------------------------------------------------------

/// 调试日志：登录窗口导航事件写入 app_local_data_dir/login_debug.log
fn debug_log(app: &AppHandle, msg: &str) {
    if let Ok(path) = store_path(app) {
        let log_path = path.with_file_name("login_debug.log");
        let line = format!(
            "[{}] {}\r\n",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            msg
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// 打开平台官方登录页窗口（用户在窗口内完成扫码/手机号登录）
///
/// async：命令在异步运行时线程执行，窗口创建即使卡住也不会阻塞主线程（避免整个应用假死）。
/// 独立 data_directory：登录窗口使用自己的 WebView2 用户数据目录，避免与主窗口
/// 共用 UDF 时创建第二个 WebView 卡住/白屏（本机已复现）。
#[tauri::command]
pub async fn open_login_window(app: AppHandle, platform: String) -> Result<String, String> {
    let cfg = platform_auth(&platform).ok_or_else(|| format!("未知平台: {}", platform))?;
    let label = format!("login-{}", platform);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
        return Ok("exists".into());
    }
    let url: tauri::Url = cfg
        .login_url
        .parse()
        .map_err(|_| "登录页 URL 无效".to_string())?;
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("login-webview");
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    debug_log(
        &app,
        &format!("open_login_window {} url={}", platform, cfg.login_url),
    );
    let nav_app = app.clone();
    let load_app = app.clone();
    let dbg_platform = platform.clone();
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
        .data_directory(data_dir)
        .title(format!("登录{}", cfg.label))
        .inner_size(1080.0, 760.0)
        .min_inner_size(800.0, 600.0)
        // 平台主页为桌面布局（最小宽度约 1200），最大化可保证右上角「登录」入口可见
        .maximized(true)
        .on_navigation(move |url| {
            debug_log(&nav_app, &format!("[{}] navigation: {}", dbg_platform, url));
            true
        })
        .on_page_load(move |win, payload| {
            debug_log(
                &load_app,
                &format!(
                    "[{}] page_load {:?}: {}",
                    win.label(),
                    payload.event(),
                    payload.url()
                ),
            );
        })
        .build()
        .map_err(|e| {
            debug_log(&app, &format!("build 失败: {}", e));
            e.to_string()
        })?;
    debug_log(&app, "build ok");
    Ok("opened".into())
}

/// 完成登录：从登录窗口读取 cookie → 持久化 → 注入适配器 → 关闭窗口
#[tauri::command]
pub async fn complete_login(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<PlatformSession, String> {
    let cfg = platform_auth(&platform).ok_or_else(|| format!("未知平台: {}", platform))?;
    let label = format!("login-{}", platform);
    let win = app
        .get_webview_window(&label)
        .ok_or_else(|| "登录窗口未打开，请先点击「打开登录窗口」".to_string())?;

    // cookie 捕获需要跨线程等待 WebView2 回调，放阻塞线程执行
    let capture_win = win.clone();
    let capture_uri = cfg.cookie_uri.to_string();
    let pairs =
        tokio::task::spawn_blocking(move || capture_cookies_blocking(&capture_win, &capture_uri))
            .await
            .map_err(|e| e.to_string())??;

    // 只保留会话相关域的 cookie（GetCookies 已按域过滤，这里再防御一次）
    let domain = cfg
        .cookie_uri
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let cookie = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ");
    if cookie.is_empty() {
        return Err(format!(
            "{} 未读取到任何 cookie，请先在登录窗口完成登录",
            cfg.label
        ));
    }
    let _ = domain;

    // 登录状态校验/用户名（B站走 nav API 真实验证；其他平台按关键 cookie 存在性判定）
    let (logged_in, username) = match platform.as_str() {
        "bilibili" => match bilibili_verify(&cookie).await {
            Some(uname) => (true, Some(uname)),
            None => (false, None),
        },
        "douyu" => {
            let nickname = pairs
                .iter()
                .find(|(k, _)| k == "acf_nickname")
                .map(|(_, v)| {
                    urlencoding::decode(v)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| v.clone())
                });
            (true, nickname)
        }
        "huya" => (true, None),
        "kuaishou" => {
            let logged_in = pairs.iter().any(|(k, _)| {
                matches!(
                    k.as_str(),
                    "kuaishou.live.web_st" | "kuaishou.server.web_st" | "userId"
                )
            });
            (logged_in, None)
        }
        _ => (false, None),
    };
    if !logged_in {
        return Err(format!(
            "{} 登录会话无效（可能登录未完成），请重试",
            cfg.label
        ));
    }

    let session = PlatformSession { cookie, username };
    // 持久化
    {
        let mut guard = auth_store().lock().unwrap();
        guard.platforms.insert(platform.clone(), session.clone());
        persist(&app, &guard)?;
    }
    // 注入适配器
    state
        .adapters
        .set_platform_cookies(&platform, Some(session.cookie.clone()));
    // 关闭登录窗口
    let _ = win.close();
    log::info!("{} 登录成功，cookie 已保存", cfg.label);
    Ok(session)
}

/// 查询平台登录状态（含快手）
#[tauri::command]
pub fn get_auth_status(app: AppHandle) -> Vec<AuthStatus> {
    let guard = auth_store().lock().unwrap();
    ["huya", "douyu", "bilibili", "kuaishou", "douyin"]
        .iter()
        .map(|p| {
            let cfg = platform_auth(p);
            let session = guard.platforms.get(*p);
            AuthStatus {
                platform: p.to_string(),
                label: cfg
                    .as_ref()
                    .map(|c| c.label.to_string())
                    // 无登录配置的平台（抖音）用固定中文标签
                    .unwrap_or_else(|| match *p {
                        "douyin" => "抖音".to_string(),
                        _ => p.to_string(),
                    }),
                logged_in: session.map(|s| !s.cookie.is_empty()).unwrap_or(false),
                // 旧版本斗鱼 nickname 曾以百分号编码保存，这里统一解码展示
                username: session
                    .and_then(|s| s.username.clone())
                    .map(|u| urlencoding::decode(&u).map(|v| v.to_string()).unwrap_or(u)),
                can_login: cfg.is_some(),
            }
        })
        .collect()
}

/// 退出登录：清除存储 + 适配器 cookie
#[tauri::command]
pub fn logout(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    platform: String,
) -> Result<(), String> {
    {
        let mut guard = auth_store().lock().unwrap();
        guard.platforms.remove(&platform);
        persist(&app, &guard)?;
    }
    state.adapters.set_platform_cookies(&platform, None);
    Ok(())
}

// ---------------------------------------------------------------------------
// cookie 捕获（Windows WebView2 专用）
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn capture_cookies_blocking(
    win: &tauri::WebviewWindow,
    uri: &str,
) -> Result<Vec<(String, String)>, String> {
    use webview2_com::{
        take_pwstr, GetCookiesCompletedHandler,
        Microsoft::Web::WebView2::Win32::{ICoreWebView2CookieList, ICoreWebView2_2},
    };
    use windows_core::{Interface, PWSTR};

    let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<(String, String)>, String>>();
    let uri_h: windows_core::HSTRING = uri.into();

    win.with_webview(move |webview| {
        let inner_tx = tx.clone();
        let uri_h = uri_h.clone();
        let result: Result<(), String> = (|| unsafe {
            let controller = webview.controller();
            let core = controller.CoreWebView2().map_err(|e| e.to_string())?;
            // CookieManager 属性在 ICoreWebView2_2
            let webview2 = core.cast::<ICoreWebView2_2>().map_err(|e| e.to_string())?;
            let manager = webview2.CookieManager().map_err(|e| e.to_string())?;

            let handler_tx = inner_tx.clone();
            // create() 返回 COM 接口；闭包为 FnOnce：(Result<()>, Option<CookieList>) -> Result<()>
            let handler = GetCookiesCompletedHandler::create(Box::new(
                move |_: windows_core::Result<()>, list: Option<ICoreWebView2CookieList>| {
                    let Some(list) = list else {
                        let _ = handler_tx.send(Err("cookie 列表为空".into()));
                        return Ok(());
                    };
                    let mut out = Vec::new();
                    let mut count: u32 = 0;
                    if list.Count(&mut count).is_ok() {
                        for i in 0..count {
                            let Ok(c) = list.GetValueAtIndex(i) else {
                                continue;
                            };
                            let (mut name, mut value) = (PWSTR::null(), PWSTR::null());
                            if c.Name(&mut name).is_ok() {
                                let n = take_pwstr(name);
                                let v = if c.Value(&mut value).is_ok() {
                                    take_pwstr(value)
                                } else {
                                    String::new()
                                };
                                if !n.is_empty() {
                                    out.push((n, v));
                                }
                            }
                        }
                    }
                    let _ = handler_tx.send(Ok(out));
                    Ok(())
                },
            ));

            manager
                .GetCookies(&uri_h, &handler)
                .map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = inner_tx.send(Err(e));
        }
    })
    .map_err(|e| e.to_string())?;

    // 回调在主线程事件循环派发，这里仅阻塞命令线程，不会死锁
    rx.recv_timeout(std::time::Duration::from_secs(8))
        .map_err(|_| "读取 cookie 超时".to_string())?
}

#[cfg(not(windows))]
fn capture_cookies_blocking(
    _win: &tauri::WebviewWindow,
    _uri: &str,
) -> Result<Vec<(String, String)>, String> {
    Err("cookie 捕获当前仅支持 Windows".into())
}

// ---------------------------------------------------------------------------
// 登录状态真实验证
// ---------------------------------------------------------------------------

/// B站 nav API：cookie 有效返回 Some(用户名)
async fn bilibili_verify(cookie: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .ok()?;
    let resp = client
        .get("https://api.bilibili.com/x/web-interface/nav")
        .header("Cookie", cookie)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await
        .ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    if json["code"].as_i64() == Some(0) {
        json["data"]["uname"].as_str().map(String::from)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 扫码登录（B站 / 斗鱼）
//
// 平台没有面向第三方桌面端的 OAuth 回调，跳系统浏览器登录后会话只留在浏览器里，
// 应用无法读取；因此登录态通过平台官方扫码接口获取：
//   1. start_qr_login 返回二维码 SVG + key
//   2. 前端轮询 poll_qr_login，直到 pending → scanned → success
//   3. 成功后在服务端解析/收集会话 cookie，持久化并注入适配器
// 虎牙的扫码接口走 UDB 私有交换协议，暂保留内嵌窗口登录。
// ---------------------------------------------------------------------------

/// 扫码登录票据
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrLoginTicket {
    pub platform: String,
    pub key: String,
    pub qr_svg: String,
}

/// 扫码登录轮询结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QrLoginPoll {
    /// pending（未扫码）| scanned（已扫码待确认）| success | expired
    pub status: String,
    pub username: Option<String>,
}

/// 进程内扫码会话：key → 平台侧请求需要携带的 cookie
struct QrSession {
    cookies: HashMap<String, String>,
}

static QR_SESSIONS: OnceLock<Mutex<HashMap<String, QrSession>>> = OnceLock::new();

fn qr_sessions() -> &'static Mutex<HashMap<String, QrSession>> {
    QR_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .user_agent(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
        )
        .build()
        .map_err(|e| e.to_string())
}

fn render_qr_svg(content: &str) -> Result<String, String> {
    use qrcode::render::svg;
    let code = qrcode::QrCode::new(content.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#172033"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

fn collect_set_cookies(jar: &mut HashMap<String, String>, headers: &reqwest::header::HeaderMap) {
    for value in headers.get_all(reqwest::header::SET_COOKIE) {
        let Ok(raw) = value.to_str() else { continue };
        let pair = raw.split(';').next().unwrap_or("").trim();
        if let Some((name, val)) = pair.split_once('=') {
            if !name.trim().is_empty() {
                jar.insert(name.trim().to_string(), val.trim().to_string());
            }
        }
    }
}

fn cookie_header(jar: &HashMap<String, String>) -> String {
    jar.iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

fn store_session(
    app: &AppHandle,
    state: &AppState,
    platform: &str,
    cookie: String,
    username: Option<String>,
) -> Result<PlatformSession, String> {
    let session = PlatformSession {
        cookie: cookie.clone(),
        username,
    };
    {
        let mut guard = auth_store().lock().unwrap();
        guard
            .platforms
            .insert(platform.to_string(), session.clone());
        persist(app, &guard)?;
    }
    state.adapters.set_platform_cookies(platform, Some(cookie));
    Ok(session)
}

#[tauri::command]
pub async fn start_qr_login(platform: String) -> Result<QrLoginTicket, String> {
    let (key, content, cookies) = match platform.as_str() {
        "bilibili" => {
            let (key, url) = bilibili_qr_start().await?;
            (key, url, HashMap::new())
        }
        "douyu" => douyu_qr_start().await?,
        _ => {
            let label = platform_auth(&platform)
                .map(|c| c.label)
                .unwrap_or("该平台");
            return Err(format!("{}暂不支持扫码登录，请使用内嵌登录窗口", label));
        }
    };
    let qr_svg = render_qr_svg(&content)?;
    qr_sessions()
        .lock()
        .unwrap()
        .insert(key.clone(), QrSession { cookies });
    Ok(QrLoginTicket {
        platform,
        key,
        qr_svg,
    })
}

#[tauri::command]
pub async fn poll_qr_login(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    platform: String,
    key: String,
) -> Result<QrLoginPoll, String> {
    match platform.as_str() {
        "bilibili" => poll_bilibili_qr(&app, state.inner(), &key).await,
        "douyu" => poll_douyu_qr(&app, state.inner(), &key).await,
        _ => Err(format!("{}暂不支持扫码登录", platform)),
    }
}

async fn bilibili_qr_start() -> Result<(String, String), String> {
    let json: serde_json::Value = http_client()?
        .get("https://passport.bilibili.com/x/passport-login/web/qrcode/generate")
        .query(&[("source", "main-fe-header")])
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await
        .map_err(|e| format!("B站二维码获取失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("B站二维码响应解析失败: {}", e))?;
    if json["code"].as_i64() != Some(0) {
        return Err(format!("B站二维码接口返回异常: {}", json["message"]));
    }
    let key = json["data"]["qrcode_key"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let url = json["data"]["url"].as_str().unwrap_or("").to_string();
    if key.is_empty() || url.is_empty() {
        return Err("B站二维码接口返回内容为空".into());
    }
    Ok((key, url))
}

async fn poll_bilibili_qr(
    app: &AppHandle,
    state: &AppState,
    key: &str,
) -> Result<QrLoginPoll, String> {
    let json: serde_json::Value = http_client()?
        .get("https://passport.bilibili.com/x/passport-login/web/qrcode/poll")
        .query(&[("qrcode_key", key), ("source", "main-fe-header")])
        .header("Referer", "https://www.bilibili.com/")
        .send()
        .await
        .map_err(|e| format!("B站扫码状态查询失败: {}", e))?
        .json()
        .await
        .map_err(|e| format!("B站扫码状态解析失败: {}", e))?;
    if json["code"].as_i64() != Some(0) {
        return Err(format!("B站扫码状态接口异常: {}", json["message"]));
    }
    let (status, username) = match json["data"]["code"].as_i64().unwrap_or(0) {
        86101 => ("pending", None),
        86090 => ("scanned", None),
        86038 => ("expired", None),
        0 => {
            let url = json["data"]["url"].as_str().unwrap_or("");
            let cookie = bilibili_cookie_from_login_url(url);
            if cookie.is_empty() {
                return Err("B站登录成功，但未解析到会话 cookie".into());
            }
            let username = bilibili_verify(&cookie).await;
            store_session(app, state, "bilibili", cookie, username.clone())?;
            qr_sessions().lock().unwrap().remove(key);
            ("success", username)
        }
        other => return Err(format!("B站扫码状态未知: {}", other)),
    };
    Ok(QrLoginPoll {
        status: status.into(),
        username,
    })
}

/// 从 B 站扫码成功回调 URL 的 query 中提取会话 cookie
fn bilibili_cookie_from_login_url(url: &str) -> String {
    const WANTED: [&str; 6] = [
        "SESSDATA",
        "bili_jct",
        "DedeUserID",
        "DedeUserID__ckMd5",
        "sid",
        "buvid3",
    ];
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for (k, v) in parsed.query_pairs() {
        if WANTED.contains(&k.as_ref()) {
            parts.push(format!("{}={}", k, v));
        }
    }
    parts.join("; ")
}

async fn douyu_qr_start() -> Result<(String, String, HashMap<String, String>), String> {
    let resp = http_client()?
        .post("https://passport.douyu.com/scan/generateCode")
        .header("Referer", "https://passport.douyu.com/member/login")
        .header("Origin", "https://passport.douyu.com")
        .form(&[("client_id", "1"), ("isMultiAccount", "0")])
        .send()
        .await
        .map_err(|e| format!("斗鱼二维码获取失败: {}", e))?;
    let mut cookies = HashMap::new();
    collect_set_cookies(&mut cookies, resp.headers());
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("斗鱼二维码响应解析失败: {}", e))?;
    if json["error"].as_i64() != Some(0) {
        return Err(format!("斗鱼二维码接口返回异常: {}", json["msg"]));
    }
    let key = json["data"]["code"].as_str().unwrap_or("").to_string();
    let url = json["data"]["url"].as_str().unwrap_or("").to_string();
    if key.is_empty() || url.is_empty() {
        return Err("斗鱼二维码接口返回内容为空".into());
    }
    Ok((key, url, cookies))
}

async fn poll_douyu_qr(
    app: &AppHandle,
    state: &AppState,
    key: &str,
) -> Result<QrLoginPoll, String> {
    let mut jar = {
        let map = qr_sessions().lock().unwrap();
        map.get(key).map(|s| s.cookies.clone()).unwrap_or_default()
    };
    let ts = chrono::Utc::now().timestamp_millis().to_string();
    let resp = http_client()?
        .get("https://passport.douyu.com/japi/scan/auth")
        .query(&[("time", ts.as_str()), ("code", key)])
        .header("Referer", "https://passport.douyu.com/member/login")
        .header("Cookie", cookie_header(&jar))
        .send()
        .await
        .map_err(|e| format!("斗鱼扫码状态查询失败: {}", e))?;
    collect_set_cookies(&mut jar, resp.headers());
    // 回写本轮新增 cookie，供后续轮询使用
    if let Some(session) = qr_sessions().lock().unwrap().get_mut(key) {
        session.cookies = jar.clone();
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("斗鱼扫码状态解析失败: {}", e))?;
    let (status, username) = match json["error"].as_i64().unwrap_or(-2) {
        0 => {
            // 登录成功回调：GET 平台返回的 url 才会下发 acf_* 会话 cookie
            if let Some(url) = json["data"]["url"].as_str().filter(|u| !u.is_empty()) {
                let resp = http_client()?
                    .get(url)
                    .header("Cookie", cookie_header(&jar))
                    .header("Referer", "https://passport.douyu.com/member/login")
                    .send()
                    .await
                    .map_err(|e| format!("斗鱼登录回调请求失败: {}", e))?;
                collect_set_cookies(&mut jar, resp.headers());
                let _ = resp.text().await;
            }
            if !jar.contains_key("acf_auth") && !jar.contains_key("acf_uid") {
                return Err("斗鱼扫码已确认，但未取到登录会话，请改用内嵌登录窗口".into());
            }
            let username = jar
                .get("acf_nickname")
                .and_then(|v| urlencoding::decode(v).ok())
                .map(|v| v.to_string());
            store_session(app, state, "douyu", cookie_header(&jar), username.clone())?;
            qr_sessions().lock().unwrap().remove(key);
            ("success", username)
        }
        1 => ("scanned", None),
        -1 => ("expired", None),
        _ => ("pending", None),
    };
    Ok(QrLoginPoll {
        status: status.into(),
        username,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilibili_login_url_cookie_parse() {
        let url = "https://passport.biligame.com/crossDomain?SESSDATA=abc%2C123&bili_jct=xyz\
                   &DedeUserID=42&DedeUserID__ckMd5=q&sid=s&gourl=x";
        let cookie = bilibili_cookie_from_login_url(url);
        assert!(cookie.contains("SESSDATA=abc,123"), "cookie: {}", cookie);
        assert!(cookie.contains("DedeUserID=42"), "cookie: {}", cookie);
        assert!(!cookie.contains("gourl"), "不应包含非会话参数: {}", cookie);
    }

    #[tokio::test]
    #[ignore = "依赖真实网络"]
    async fn bilibili_qr_start_smoke() {
        let (key, url) = bilibili_qr_start().await.expect("B站二维码生成失败");
        assert!(!key.is_empty());
        assert!(url.starts_with("https://"));
    }

    #[tokio::test]
    #[ignore = "依赖真实网络"]
    async fn douyu_qr_start_smoke() {
        let (key, url, _) = douyu_qr_start().await.expect("斗鱼二维码生成失败");
        assert!(!key.is_empty());
        assert!(url.starts_with("https://"));
    }
}
