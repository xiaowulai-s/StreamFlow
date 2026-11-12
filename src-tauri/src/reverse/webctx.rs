//! 抖音页面环境搜索（隐藏登录窗口架构）
//!
//! 实验结论（2026-11-12）：抖音 web 接口对独立 HTTP 客户端一律降级空数据（缺浏览器上下文），
//! 唯一可行路径是**在真实 WebView 页面环境里发起 fetch**（credentials: 'include'，
//! 服务端认可同源浏览器上下文，无需 a_bogus/msToken——实测登录态 code=0 且返回 8 条结果）。
//!
//! 数据流：douyin.rs search → 本模块 → eval 注入 JS 到隐藏的 login-douyin 窗口
//! → 页面内 fetch 抖音接口 → POST 结果到本地 listener → search 命令取回解析。

use std::sync::{Arc, Condvar, Mutex};
use serde_json::Value;
use tauri::{AppHandle, Manager};

/// 本地回传 listener 端口（bind 失败自动+1 重试，最多 10 次）
static LISTEN_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();
/// 全局 AppHandle（main.rs setup 时设置）
static APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

/// 结果槽：eval 的 JS POST 回来的响应体
struct ResultSlot {
    body: Mutex<Option<String>>,
    signal: Condvar,
}
static SLOT: std::sync::OnceLock<ResultSlot> = std::sync::OnceLock::new();

fn slot() -> &'static ResultSlot {
    SLOT.get_or_init(|| ResultSlot { body: Mutex::new(None), signal: Condvar::new() })
}

/// main.rs setup 时调用：保存 AppHandle 并启动本地 listener
pub fn init(app: AppHandle) {
    let _ = APP.set(app);
    // 端口扫描：18488 起
    for port in 18488..18498 {
        if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) {
            let _ = LISTEN_PORT.set(port);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    handle_conn(&mut stream);
                }
            });
            log::info!("[webctx] listener on 127.0.0.1:{}", port);
            return;
        }
    }
    log::warn!("[webctx] listener 绑定失败（18488-18497 均被占用），页面环境搜索不可用");
}

/// 读取完整请求（按 Content-Length 动态扩容），存入结果槽
fn handle_conn(stream: &mut std::net::TcpStream) {
    use std::io::Read;
    let mut buf: Vec<u8> = vec![0u8; 16384];
    let mut total = 0usize;
    let mut target: Option<usize> = None; // head_end + content_length

    loop {
        if total == buf.len() {
            buf.resize(buf.len() * 2, 0);
        }
        match stream.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if target.is_none() {
                    if let Some(pos) = buf[..total].windows(4).position(|w| w == b"\r\n\r\n") {
                        let cl: usize = String::from_utf8_lossy(&buf[..pos])
                            .lines()
                            .find(|l| l.to_lowercase().starts_with("content-length:"))
                            .and_then(|l| l.split(':').nth(1))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        target = Some(pos + 4 + cl);
                    }
                }
                if let Some(t) = target {
                    if total >= t {
                        break;
                    }
                    if buf.len() < t {
                        buf.resize(t + 4096, 0);
                    }
                }
            }
            Err(_) => break,
        }
    }

    // 提取 body（POST /result）
    let text = String::from_utf8_lossy(&buf[..total]).to_string();
    if let Some(pos) = text.find("\r\n\r\n") {
        if text.starts_with("POST /diag") { log::info!("[webctx][diag] {}", text[pos+4..].chars().take(100).collect::<String>()); } else if text.starts_with("POST /result") {
            let body = text[pos + 4..].to_string();
            // PING 是通路探针，不写入结果槽
            if body != "PING" {
                let mut guard = slot().body.lock().unwrap();
                *guard = Some(body);
                slot().signal.notify_all();
            }
        }
    }
    // 响应 CORS（页面在 douyin.com，跨域 POST 需要）
    let resp = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: 2\r\n\r\nok";
    use std::io::Write;
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn port() -> u16 {
    *LISTEN_PORT.get().unwrap_or(&18488)
}

/// 惰性创建隐藏的抖音页面窗口（与登录窗口共用 data_directory，共享登录 cookie）
async fn ensure_window(app: &AppHandle) -> anyhow::Result<tauri::WebviewWindow> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .join("login-webview");
    std::fs::create_dir_all(&data_dir).map_err(|e| anyhow::anyhow!("{}", e))?;
    let url: tauri::Url = "https://www.douyin.com/"
        .parse()
        .map_err(|_| anyhow::anyhow!("URL 无效"))?;
    let win = WebviewWindowBuilder::new(app, "login-douyin", WebviewUrl::External(url))
        .data_directory(data_dir)
        .title("抖音页面环境")
        .inner_size(1100.0, 720.0)
        .visible(false)
        .build()
        .map_err(|e| anyhow::anyhow!("创建页面窗口失败: {}", e))?;
    // 轮询等页面进入抖音域
    for _ in 0..24 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let u = win.url().map(|u| u.to_string()).unwrap_or_default();
        if u.contains("douyin.com") {
            // 再给页面 JS 初始化时间
            tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
            log::info!("[webctx] 隐藏页面窗口就绪: {}", u);
            return Ok(win);
        }
    }
    anyhow::bail!("页面窗口加载超时（12s 内未进入抖音域）")
}

/// 等待结果槽（清空 + 阻塞至填充或超时）
fn wait_result(timeout: std::time::Duration) -> Option<String> {
    let mut guard = slot().body.lock().unwrap();
    let start = std::time::Instant::now();
    loop {
        if let Some(b) = guard.take() {
            return Some(b);
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return None;
        }
        let (g, _) = slot()
            .signal
            .wait_timeout(guard, timeout - elapsed)
            .unwrap();
        guard = g;
    }
}

/// 在隐藏的抖音页面环境里执行 fetch 搜索，返回 JSON 文本
///
/// 页面来源：`login-douyin` 窗口（登录成功后 hide 保留）。窗口不存在时
/// 惰性创建隐藏窗口（同登录 data_directory → 共享登录 cookie，应用重启后自动恢复）。
pub async fn live_search(keyword: &str) -> anyhow::Result<Vec<serde_json::Value>> {
    let app = APP.get().ok_or_else(|| anyhow::anyhow!("webctx 未初始化"))?;
    let win = match app.get_webview_window("login-douyin") {
        Some(w) => w,
        None => ensure_window(app).await?,
    };

    // 页面必须在抖音域（用户可能在窗口里手动导航走了）
    let url = win.url().map(|u| u.to_string()).unwrap_or_default();
    if !url.contains("douyin.com") {
        use tauri::Manager;
        let _ = win.eval("location.href='https://live.douyin.com/'");
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let u = win.url().map(|u| u.to_string()).unwrap_or_default();
            if u.contains("douyin.com") {
                break;
            }
        }
        // 页面加载后给 JS 一点初始化时间
        tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
    }

    // 构造参数串（与 Playwright 实验一致的参数集；不含 msToken/a_bogus——页面环境不需要）
    let params = format!(
        "device_platform=webapp&aid=6383&channel=channel_pc_web&search_channel=aweme_live&keyword={kw}\
         &search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=15\
         &pc_client_type=1&version_code=170400&version_name=17.4.0&cookie_enabled=true&screen_width=1920\
         &screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome\
         &browser_version=130.0.0.0&browser_online=true&engine_name=Blink&engine_version=130.0.0.0\
         &os_name=Windows&os_version=10&cpu_core_num=12&memory_info=8&platform=PC&downlink=10\
         &effective_type=4g&round_trip_time=50",
        kw = urlencoding::encode(keyword)
    );
    let api_url = format!("https://www.douyin.com/aweme/v1/web/live/search/?{}", params);
    let cb = format!("http://127.0.0.1:{}/result", port());

    // 清空结果槽
    {
        let mut g = slot().body.lock().unwrap();
        *g = None;
    }

    // 注入 fetch（credentials include，结果以 no-cors POST 回 listener——绕过页面 CORS/preflight）
    let js = format!(
        r#"(async () => {{
            const send = (body) => fetch("{cb}", {{ method: "POST", mode: "no-cors", body }});
            try {{
                const r = await fetch("{url}", {{ credentials: "include" }});
                const t = await r.text();
                await send(t);
            }} catch (e) {{
                try {{ await send("E:" + e.message); }} catch (_) {{}}
            }}
        }})()"#, 
        url = api_url.replace('"', "%22"),
        cb = cb,
    );
    win.eval(&js)
        .map_err(|e| anyhow::anyhow!("eval 注入失败: {}", e))?;

    let body = wait_result(std::time::Duration::from_secs(12))
        .ok_or_else(|| anyhow::anyhow!("页面环境搜索超时（12s）"))?;

    if let Some(err) = body.strip_prefix("E:") {
        anyhow::bail!("页面 fetch 失败: {}", err);
    }
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| anyhow::anyhow!("响应非 JSON: {}", e))?;
    let code = json.get("status_code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        anyhow::bail!(
            "搜索接口 status_code={}: {}",
            code,
            json.get("status_msg").and_then(|v| v.as_str()).unwrap_or("unknown")
        );
    }
    let list = json
        .pointer("/data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
 