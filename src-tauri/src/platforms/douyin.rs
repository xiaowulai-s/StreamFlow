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
use std::sync::OnceLock;
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
}

impl DouyinAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                .build()
                .unwrap(),
        }
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
    let live = json_i64(room.get("status").unwrap_or(&Value::Null)) == Some(2);
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

        // 关键词搜索需 a_bogus 签名：暂用热门 feed 做有限匹配
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

    /// 热门直播间（webcast feed，游客可访问）
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let items = self.fetch_feed(20).await?;
        Ok(items
            .iter()
            .filter(|item| json_i64(item.get("status").unwrap_or(&Value::Null)) == Some(2))
            .filter_map(room_info_from_room)
            .take(20)
            .collect())
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
