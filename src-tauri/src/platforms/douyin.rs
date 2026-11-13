//! Douyin 抖音适配器
//!
//! 实测结论（2026-11）：
//! - `live.douyin.com/webcast/room/web/enter/` 对非浏览器请求返回空 body，已不可用；
//! - `webcast.amemv.com/webcast/room/reflow/info/?app_id=1128&live_id=1&room_id=...`
//!   游客可直接拿到房间 JSON（status_code=0），含标题/主播/封面/观看数/流地址；
//! - `https://live.douyin.com/{web_rid}` 页面内嵌数据里含真实 `roomId`，用于把
//!   web_rid（直播间链接里的短号）解析成房间号；
//! - 流地址取 `stream_url.hls_pull_url_map`（HLS），把 http 换成 https 后
//!   WebView2 可正常播放。
//!
//! 限制：抖音关键词搜索依赖 `a_bogus` 签名，暂未逆向；关键词搜索退化为
//! 在热门 feed 中做标题/主播名匹配，房间号与直播间链接可直接直达。

use crate::{AdapterError, AdapterResult, LiveStatus, QualityOption, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REFLOW_INFO_URL: &str = "https://webcast.amemv.com/webcast/room/reflow/info/";
const FEED_URL: &str = "https://live.douyin.com/webcast/feed/";
const ROOM_PAGE_PREFIX: &str = "https://live.douyin.com/";

/// 平台统一放在这里，避免各处硬编码
const PLATFORM: &str = "douyin";

#[derive(Clone)]
pub struct DouyinAdapter {
    client: Client,
    cookies: Arc<RwLock<Option<String>>>,
}

impl DouyinAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .use_rustls_tls()
                .build()
                .unwrap(),
            cookies: Arc::new(RwLock::new(None)),
        }
    }

    /// 当前持有的登录 cookie（无则为 None）
    fn current_cookie(&self) -> Option<String> {
        self.cookies.read().unwrap().clone().filter(|c| !c.is_empty())
    }

    /// 访问直播首页拿游客 ttwid（搜索接口需要）
    async fn fetch_ttwid(&self) -> Option<String> {
        let resp = self
            .client
            .get("https://live.douyin.com/")
            .header("User-Agent", crate::reverse::douyin_abogus::USER_AGENT)
            .send()
            .await
            .ok()?;
        let cookies: Vec<String> = resp
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .map(|s| s.split(';').next().unwrap_or("").to_string())
            .filter(|s| s.starts_with("ttwid="))
            .collect();
        if cookies.is_empty() {
            None
        } else {
            Some(cookies.join("; "))
        }
    }

    /// 登录态关键词搜索：/aweme/v1/web/live/search/（a_bogus 签名 + 登录 cookie）
    /// 失败时返回 Err，由调用方降级到热门 feed 匹配。
    async fn web_live_search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        let login_cookie = self.current_cookie();
        let ttwid = self.fetch_ttwid().await.unwrap_or_default();
        let mut cookie_header = ttwid;
        if let Some(c) = &login_cookie {
            if !cookie_header.is_empty() {
                cookie_header.push_str("; ");
            }
            cookie_header.push_str(c);
        }

        let params = format!(
            "device_platform=webapp&aid=6383&channel=channel_pc_web&search_channel=aweme_live&keyword={kw}\
             &search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=15\
             &pc_client_type=1&version_code=170400&version_name=17.4.0&cookie_enabled=true&screen_width=1920\
             &screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome\
             &browser_version=130.0.0.0&browser_online=true&engine_name=Blink&engine_version=130.0.0.0\
             &os_name=Windows&os_version=10&cpu_core_num=12&memory_info=8&platform=PC&downlink=10\
             &effective_type=4g&round_trip_time=50&webid=7300000000000000000",
            kw = urlencoding::encode(keyword)
        );
        let a_bogus = crate::reverse::douyin_abogus::get_value(&params, "GET");
        let url = format!(
            "https://www.douyin.com/aweme/v1/web/live/search/?{}&a_bogus={}",
            params,
            urlencoding::encode(&a_bogus)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", crate::reverse::douyin_abogus::USER_AGENT)
            .header("Referer", format!("https://www.douyin.com/search/{}", urlencoding::encode(keyword)))
            .header("Cookie", cookie_header)
            .send()
            .await?
            .error_for_status()?;
        let json: Value = resp.json().await?;
        let code = json_i64(json.get("status_code").unwrap_or(&Value::Null));
        if code != Some(0) {
            anyhow::bail!(
                "搜索接口 status_code={:?}: {}",
                code,
                json.get("status_msg").and_then(|v| v.as_str()).unwrap_or("unknown")
            );
        }
        let list = json
            .pointer("/data/aweme_list")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let rooms: Vec<RoomInfo> = list.iter().filter_map(parse_live_search_item).collect();
        Ok(rooms)
    }

    /// 当前抖音房间号是 19 位数字；较短的纯数字按 web_rid 处理
    fn looks_like_room_id(s: &str) -> bool {
        s.len() >= 17 && s.chars().all(|c| c.is_ascii_digit())
    }

    /// 从用户输入中提取房间号 / web_rid / 链接中的数字段
    fn clean_input(input: &str) -> String {
        let trimmed = input.trim();
        let lower = trimmed.to_lowercase();
        let rest = if let Some(pos) = lower.find("live.douyin.com/") {
            &trimmed[pos + "live.douyin.com/".len()..]
        } else if let Some(pos) = lower.find("/reflow/") {
            &trimmed[pos + "/reflow/".len()..]
        } else {
            trimmed
        };
        rest.split(['?', '#', '/', ' '])
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// 房间 JSON（reflow info，游客可用）
    async fn fetch_reflow(&self, room_id: &str) -> anyhow::Result<Value> {
        let resp = self
            .client
            .get(REFLOW_INFO_URL)
            .query(&[
                ("app_id", "1128"),
                ("live_id", "1"),
                ("room_id", room_id),
            ])
            .header("User-Agent", USER_AGENT)
            .header("Referer", format!("{ROOM_PAGE_PREFIX}{room_id}"))
            .send()
            .await?
            .error_for_status()?;
        let json: Value = resp.json().await?;
        if json.get("status_code").and_then(Value::as_i64) != Some(0) {
            let msg = json
                .pointer("/data/message")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            anyhow::bail!("抖音房间 {} 获取失败：{}", room_id, msg);
        }
        json.pointer("/data/room")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("抖音房间 {} 响应缺少 room 数据", room_id))
    }

    /// 抓直播间页面（web_rid 页面数据里含真实 roomId）
    async fn fetch_room_page(&self, web_rid: &str) -> anyhow::Result<String> {
        let resp = self
            .client
            .get(format!("{ROOM_PAGE_PREFIX}{web_rid}"))
            .header("User-Agent", USER_AGENT)
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.text().await?)
    }

    /// 把用户输入解析成真实房间号（弹幕模块也复用）
    pub(crate) async fn resolve_room_id(&self, input: &str) -> anyhow::Result<String> {
        let cleaned = Self::clean_input(input);
        if cleaned.is_empty() {
            anyhow::bail!("请输入抖音房间号或直播间链接");
        }
        if Self::looks_like_room_id(&cleaned) {
            return Ok(cleaned);
        }

        // web_rid：直播间链接里的短号，页面内嵌数据可解析出真实 roomId
        if let Ok(html) = self.fetch_room_page(&cleaned).await {
            if let Some(id) = extract_room_id_from_html(&html) {
                log::info!("抖音 web_rid {} → 房间号 {}", cleaned, id);
                return Ok(id);
            }
        }

        // 兜底：较短的旧房间号可能可直接查询
        if let Ok(room) = self.fetch_reflow(&cleaned).await {
            if let Some(id) = room_id_str(&room) {
                return Ok(id);
            }
        }
        anyhow::bail!(
            "未能在抖音直播间 {} 中解析到房间号（房间可能不存在或已下播）",
            cleaned
        )
    }

    async fn fetch_room(&self, input: &str) -> anyhow::Result<Value> {
        let room_id = self.resolve_room_id(input).await?;
        self.fetch_reflow(&room_id).await
    }

    /// 热门 feed（无需登录，实测直接返回房间列表）
    async fn fetch_feed(&self, count: usize) -> anyhow::Result<Vec<Value>> {
        let count = count.to_string();
        let resp = self
            .client
            .get(FEED_URL)
            .query(&[
                ("aid", "6383"),
                ("app_name", "douyin_web"),
                ("live_id", "1"),
                ("device_platform", "web"),
                ("count", count.as_str()),
                ("type", "0"),
            ])
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://live.douyin.com/")
            .send()
            .await?
            .error_for_status()?;
        let json: Value = resp.json().await?;
        Ok(json
            .pointer("/data/data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }
}

impl Default for DouyinAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 数字/字符串统一取 i64（抖音部分字段类型不稳定）
fn json_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn room_id_str(room: &Value) -> Option<String> {
    room.get("id_str")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            room.get("id")
                .and_then(Value::as_u64)
                .map(|id| id.to_string())
        })
}

/// 从直播间页面 HTML 中解析真实房间号
/// 数据是 RSC 转义 JSON，先还原 `\"` 再匹配 `"roomId":"<数字>"`
fn extract_room_id_from_html(html: &str) -> Option<String> {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r#""roomId"\s*:\s*"(\d{6,})""#).unwrap());
    let unescaped = html.replace("\\\"", "\"");
    re.captures(&unescaped).map(|c| c[1].to_string())
}

/// 观看人数：优先房间展示值（如 8417 / 1.2万），其次 stats.total_user
fn parse_view_count(room: &Value) -> u64 {
    if let Some(display) = room
        .pointer("/room_view_stats/display_value")
        .and_then(Value::as_str)
    {
        if let Some(n) = parse_human_count(display) {
            return n;
        }
    }
    room.pointer("/stats/total_user")
        .and_then(Value::as_u64)
        .or_else(|| room.get("user_count").and_then(Value::as_u64))
        .or_else(|| {
            // reflow 部分版本只有 user_count_str（如 "1.2万" / "8417"）
            room.get("user_count_str")
                .and_then(Value::as_str)
                .and_then(parse_human_count)
        })
        .unwrap_or(0)
}

fn parse_human_count(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix('万') {
        return v.parse::<f64>().ok().map(|n| (n * 10_000.0) as u64);
    }
    if let Some(v) = s.strip_suffix('亿') {
        return v.parse::<f64>().ok().map(|n| (n * 100_000_000.0) as u64);
    }
    s.parse::<u64>().ok()
}

fn normalize_https(url: &str) -> String {
    url.strip_prefix("http://")
        .map(|rest| format!("https://{rest}"))
        .unwrap_or_else(|| url.to_string())
}

/// reflow/feed 的房间结构一致，统一映射
/// 解析 web/live/search 返回的 aweme_list 项。
/// 实测结构（2026-11-12）：每项为 {lives: <aweme 对象>, related_config, type}，
/// 直播卡片 lives.aweme_type=101，group_id 即直播间房间 id（19 位）。
/// 搜索结果项：{lives: <aweme>, type} 包装
fn parse_live_search_item(item: &Value) -> Option<RoomInfo> {
    // 兼容直接返回房间对象（无 lives 包装）的旧结构
    if item.get("lives").is_none() && room_id_str(item).is_some() {
        return room_info_from_room(item);
    }
    parse_aweme_live(item.get("lives")?)
}

/// 关注 feed 项：{aweme: <aweme>, ...} 包装
fn parse_follow_feed_item(item: &Value) -> Option<RoomInfo> {
    parse_aweme_live(item.get("aweme")?)
}

/// 通用：从候选接口返回的项数组中解析直播房间（webcast 房间对象 / aweme 直播卡片，兼容多种包装）
fn items_to_rooms(items: &[Value]) -> Vec<RoomInfo> {
    items
        .iter()
        .filter_map(|item| {
            // 形态 A：{data: <webcast 房间>}（webcast/feed）
            item.get("data")
                .and_then(room_info_from_room)
                // 形态 B：{aweme: <直播 aweme>}（aweme feed）
                .or_else(|| parse_aweme_live(item.get("aweme")?))
                // 形态 C：{lives: <直播 aweme>}（live/search）
                .or_else(|| parse_aweme_live(item.get("lives")?))
                // 形态 D：平铺房间对象
                .or_else(|| room_info_from_room(item))
        })
        .collect()
}

/// 解析直播 aweme 对象（搜索/关注 feed 共用核心）。
/// 直播卡片两种形态：aweme_type=101（搜索），或带 anchor 字段（关注 feed 的直播引流卡，type=0）。
fn parse_aweme_live(aweme: &Value) -> Option<RoomInfo> {
    let is_live_card = aweme.get("aweme_type").and_then(Value::as_i64) == Some(101)
        || aweme.get("anchor").is_some();
    if !is_live_card {
        return None;
    }
    // 房间号：author.web_rid（最稳，直达 reflow；实测为数字型）优先，group_id 兜底
    let val_to_id = |v: &Value| -> Option<String> {
        v.as_str()
            .map(str::to_string)
            .or_else(|| v.as_i64().map(|n| n.to_string()))
            .filter(|s| !s.is_empty())
    };
    let id = aweme
        .pointer("/author/web_rid")
        .or_else(|| aweme.pointer("/author/room_id_str"))
        .or_else(|| aweme.pointer("/author/room_id"))
        .and_then(val_to_id)
        .or_else(|| {
            aweme
                .get("group_id")
                .and_then(val_to_id)
        })
        .or_else(|| aweme.get("aweme_id").and_then(val_to_id))?;
    if id.is_empty() {
        return None;
    }
    let username = aweme
        .pointer("/author/nickname")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("未知主播")
        .to_string();
    let title = aweme
        .pointer("/video/desc")
        .or_else(|| aweme.pointer("/video_text"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{} 的直播", username));
    let cover = aweme
        .pointer("/video/cover/url_list/0")
        .or_else(|| aweme.pointer("/video/big_thumbs/url_list/0"))
        .or_else(|| aweme.pointer("/video/origin_cover/url_list/0"))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some(RoomInfo {
        id,
        platform: PLATFORM.to_string(),
        title,
        username,
        view_count: 0,
        status: LiveStatus::Live,
        cover: crate::adapter::normalize_cover_url(cover),
        started_at: None,
    })
}

fn room_info_from_room(room: &Value) -> Option<RoomInfo> {
    let id = room_id_str(room)?;
    let title = room
        .get("title")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("抖音直播")
        .to_string();
    let username = room
        .pointer("/owner/nickname")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("未知主播")
        .to_string();
    let cover = room
        .pointer("/cover/url_list/0")
        .and_then(Value::as_str)
        .map(str::to_string);
    // webcast 房间对象：status=0 为直播中（与 aweme 的 2 相反）
    let live = json_i64(room.get("status").unwrap_or(&Value::Null)) == Some(2)
        || json_i64(room.get("status").unwrap_or(&Value::Null)) == Some(0);
    let started_at = room
        .get("start_time")
        .and_then(Value::as_i64)
        .filter(|v| *v > 0)
        .or_else(|| room.get("create_time").and_then(Value::as_i64));

    Some(RoomInfo {
        id,
        platform: PLATFORM.to_string(),
        title,
        username,
        view_count: parse_view_count(room),
        status: if live {
            LiveStatus::Live
        } else {
            LiveStatus::Sleeping
        },
        cover: crate::adapter::normalize_cover_url(cover),
        started_at,
    })
}

const QUALITY_ORDER: [&str; 4] = ["FULL_HD1", "HD1", "SD1", "SD2"];

fn quality_label(key: &str) -> &'static str {
    match key {
        "FULL_HD1" => "原画",
        "HD1" => "高清",
        "SD1" => "标清",
        _ => "流畅",
    }
}

/// 提取 HLS 流（优先），没有 HLS 时回退 FLV
fn collect_qualities(room: &Value) -> Vec<QualityOption> {
    let stream = &room["stream_url"];
    let default_res = stream
        .get("default_resolution")
        .and_then(Value::as_str)
        .unwrap_or("HD1");

    let mut keys: Vec<&str> = vec![default_res];
    for key in QUALITY_ORDER {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }

    let mut qualities = Vec::new();
    if let Some(map) = stream.get("hls_pull_url_map").and_then(Value::as_object) {
        for key in &keys {
            if let Some(url) = map.get(*key).and_then(Value::as_str) {
                qualities.push(QualityOption {
                    label: quality_label(key).to_string(),
                    url: normalize_https(url),
                });
            }
        }
    }
    if qualities.is_empty() {
        if let Some(map) = stream.get("flv_pull_url").and_then(Value::as_object) {
            for key in &keys {
                if let Some(url) = map.get(*key).and_then(Value::as_str) {
                    qualities.push(QualityOption {
                        label: quality_label(key).to_string(),
                        url: normalize_https(url),
                    });
                }
            }
        }
    }
    qualities
}

#[async_trait]
impl crate::adapter::PlatformAdapter for DouyinAdapter {
    fn name(&self) -> &'static str {
        PLATFORM
    }

    fn set_cookies(&self, cookie: Option<String>) {
        *self.cookies.write().unwrap() = cookie.filter(|c| !c.is_empty());
    }

    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        crate::adapter::PlatformCapability::SEARCH
            | crate::adapter::PlatformCapability::ROOM_DETAIL
            | crate::adapter::PlatformCapability::LIVE_STATUS
            | crate::adapter::PlatformCapability::STREAM_URLS
    }

    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        let kw = keyword.trim();
        if kw.is_empty() {
            return Ok(vec![]);
        }

        // 房间号 / web_rid / 直播间链接直达
        let cleaned = Self::clean_input(kw);
        if !cleaned.is_empty() && cleaned.chars().all(|c| c.is_ascii_digit()) {
            match self.room_detail(&cleaned).await {
                Ok(info) => return Ok(vec![info]),
                Err(e) => log::info!("抖音直达 {} 失败：{}", cleaned, e),
            }
        }

        // 关键词搜索：登录态走真实搜索接口（a_bogus 签名）；
        // 未登录/失败时降级热门 feed 做有限匹配
        let logged_in = self.current_cookie().is_some();
        if logged_in {
            // 优先页面环境 fetch（唯一有效通路，见 reverse/webctx.rs 模块注释）
            match crate::reverse::webctx::live_search(kw).await {
                Ok(list) => {
                    let rooms: Vec<RoomInfo> = list.iter().filter_map(parse_live_search_item).collect();
                    if !rooms.is_empty() {
                        return Ok(rooms);
                    }
                    log::info!("抖音页面环境搜索无结果，回退热门匹配");
                }
                Err(e) => log::warn!("抖音页面环境搜索失败（{}），回退热门匹配", e),
            }
        }
        let lower = kw.to_lowercase();
        match self.fetch_feed(30).await {
            Ok(items) => Ok(items
                .iter()
                .filter(|item| json_i64(item.get("status").unwrap_or(&Value::Null)) == Some(2))
                .filter_map(room_info_from_room)
                .filter(|room| {
                    room.title.to_lowercase().contains(&lower)
                        || room.username.to_lowercase().contains(&lower)
                })
                .collect()),
            Err(e) => {
                log::warn!("抖音关键词搜索失败：{}", e);
                Ok(vec![])
            }
        }
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let room = self.fetch_room(room_id).await?;
        room_info_from_room(&room)
            .ok_or_else(|| anyhow::anyhow!("抖音房间 {} 响应缺少 id", room_id))
    }

    async fn stream_urls(&self, room_id: &str) -> AdapterResult {
        let fallback = AdapterError::ExternalOnly {
            url: format!("{ROOM_PAGE_PREFIX}{room_id}"),
        };
        let room = match self.fetch_room(room_id).await {
            Ok(room) => room,
            Err(e) => {
                log::warn!("抖音流获取失败（{}）", e);
                return Err(fallback);
            }
        };
        if json_i64(room.get("status").unwrap_or(&Value::Null)) != Some(2) {
            return Err(fallback);
        }
        let qualities = collect_qualities(&room);
        if qualities.is_empty() {
            return Err(fallback);
        }
        Ok(StreamResult {
            platform: PLATFORM.to_string(),
            room_id: room_id_str(&room).unwrap_or_else(|| room_id.to_string()),
            url: qualities.iter().map(|q| q.url.clone()).collect(),
            quality: qualities,
        })
    }

    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus> {
        let room = self.fetch_room(room_id).await?;
        Ok(
            if json_i64(room.get("status").unwrap_or(&Value::Null)) == Some(2) {
                LiveStatus::Live
            } else {
                LiveStatus::Sleeping
            },
        )
    }

    /// 热门直播间：webcast feed（页面环境，游客可用——独立客户端被风控降级）
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        // 页面环境优先；失败/无结果时回退旧游客 feed（可能被风控降级空数据）
        if let Ok(items) = crate::reverse::webctx::live_recommend().await {
            let rooms: Vec<RoomInfo> = items
                .iter()
                .filter_map(|item| {
                    // data[i].data = webcast 房间对象；兼容平铺
                    item.get("data")
                        .and_then(room_info_from_room)
                        .or_else(|| room_info_from_room(item))
                })
                .take(20)
                .collect();
            if !rooms.is_empty() {
                return Ok(rooms);
            }
            log::info!("抖音页面环境热门无结果，回退游客 feed");
        }
        let items = self.fetch_feed(20).await?;
        Ok(items
            .iter()
            .filter(|item| json_i64(item.get("status").unwrap_or(&Value::Null)) == Some(2))
            .filter_map(room_info_from_room)
            .take(20)
            .collect())
    }

    /// 平台关注的主播（页面环境需登录态）：follow_top 为关注页"直播中"列表（首选），
    /// 其余候选兜底，只收直播中的卡片
    async fn follows(&self) -> anyhow::Result<Vec<RoomInfo>> {
        // 候选 0（首选）：关注页顶部"直播中"列表（webcast 房间对象，含 user_count）
        let follow_top = "https://www.douyin.com/webcast/feed/follow_top/?device_platform=webapp&aid=6383&channel=channel_pc_web&pc_client_type=1&version_code=170400&version_name=17.4.0&cookie_enabled=true&screen_width=1920&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=130.0.0.0&browser_online=true&engine_name=Blink&engine_version=130.0.0.0&os_name=Windows&os_version=10&cpu_core_num=12&memory_info=8&platform=PC&downlink=10&effective_type=4g&round_trip_time=50";
        if let Ok(body) = crate::reverse::webctx::fetch_page(follow_top).await {
            if let Ok(json) = serde_json::from_str::<Value>(&body) {
                let arr = json
                    .pointer("/data")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let rooms: Vec<RoomInfo> = items_to_rooms(&arr);
                log::info!("[webctx] follow_top 返回 {} 项，解析出 {} 个在播房间", arr.len(), rooms.len());
                // 空列表=关注的主播都没在播（正常），有数据则直接返回
                if json.pointer("/data").and_then(Value::as_array).is_some() && !arr.is_empty() {
                    return Ok(rooms);
                }
                if arr.is_empty() {
                    // 空结果疑缺签名参数（a_bogus/webid/uifid），诊断页面环境
                    let _ = crate::reverse::webctx::diag_env().await;
                }
            }
        }
        // 其余候选：pull_type 变体与混合 feed 兜底
        let candidates = [
            "https://www.douyin.com/aweme/v1/web/follow/feed/?aid=6383&count=20&cursor=0&feed_type=live",
        ];
        for url in candidates.iter() {
            if let Ok(body) = crate::reverse::webctx::fetch_page(url).await {
                if let Ok(json) = serde_json::from_str::<Value>(&body) {
                    let arr = json
                        .pointer("/data")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let rooms: Vec<RoomInfo> = items_to_rooms(&arr);
                    if !rooms.is_empty() {
                        return Ok(rooms);
                    }
                }
            }
        }
        // 兜底：关注内容混合 feed——cursor 翻页拉满（最多 5 页/100 条），收集全部直播卡
        let mut rooms: Vec<RoomInfo> = Vec::new();
        let mut cursor: u64 = 0;
        for _page in 0..5 {
            let (items, next) = match crate::reverse::webctx::follow_feed(cursor).await {
                Ok(v) => v,
                Err(e) => {
                    if rooms.is_empty() {
                        anyhow::bail!("关注 feed 拉取失败: {}", e);
                    }
                    break;
                }
            };
            rooms.extend(items.iter().filter_map(parse_follow_feed_item));
            if next == 0 || next == cursor || items.is_empty() {
                break;
            }
            cursor = next;
        }
        // 按房间号去重
        let mut seen = std::collections::HashSet::new();
        rooms.retain(|r| seen.insert(r.id.clone()));
        if rooms.is_empty() {
            anyhow::bail!("关注的主播当前没有直播中的（或登录态失效）");
        }
        Ok(rooms)
    }

    /// 抖音弹幕协议需要签名握手，暂未接入
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::PlatformAdapter;

    /// 网络集成测试：真实房间详情（需要网络）
    #[tokio::test]
    #[ignore]
    async fn room_detail_by_room_id() {
        let adapter = DouyinAdapter::new();
        let room = adapter
            .room_detail("7706776263012207410")
            .await
            .expect("抖音房间详情失败");
        println!("抖音房间: {:?}", room);
        match adapter.stream_urls("7706776263012207410").await {
            Ok(stream) => println!("HLS: {}", stream.url[0]),
            Err(e) => println!("流获取失败: {:?}", e),
        }
    }

    /// web_rid 页面解析
    #[tokio::test]
    #[ignore]
    async fn resolve_web_rid() {
        let adapter = DouyinAdapter::new();
        let id = adapter.resolve_room_id("547977714661").await.expect("解析失败");
        println!("web_rid → {}", id);
        assert!(id.len() >= 17);
    }
}
