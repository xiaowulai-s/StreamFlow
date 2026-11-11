//! Bilibili 平台适配器
//! 公开 API（实测无需登录、无需 WBI 签名）：
//! - 房间信息:  GET room/v1/Room/get_info?room_id={id}   （接受短号，返回真实 room_id）
//! - 主播名:    GET live_user/v1/Master/info?uid={uid}
//! - 流地址:    GET xlive/web-room/v2/index/getRoomPlayInfo
//!              完整流地址 = codec.url_info[0].host + codec.base_url + codec.url_info[0].extra
//!              优先 http_hls/ts 的 m3u8（实测拉流 200），兜底 flv
//! 搜索接口被风控（412），关键词作房间号直达。

use crate::{AdapterError, AdapterResult, LiveStatus, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REFERER: &str = "https://live.bilibili.com/";

#[derive(Clone)]
pub struct BilibiliAdapter {
    client: Client,
}

impl BilibiliAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }

    /// 房间信息（live_status: 0=未开播 1=直播中 2=轮播中）
    async fn fetch_room_info(&self, room_id: &str) -> anyhow::Result<serde_json::Value> {
        let url = format!(
            "https://api.live.bilibili.com/room/v1/Room/get_info?room_id={}",
            room_id
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;

        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("unknown");
            anyhow::bail!("get_info 返回错误 {}: {}", code, msg);
        }
        Ok(json)
    }

    /// 主播名（失败不影响主流程）
    async fn fetch_uname(&self, uid: u64) -> Option<String> {
        let url = format!("https://api.live.bilibili.com/live_user/v1/Master/info?uid={}", uid);
        let json = self
            .client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .ok()?
            .json::<serde_json::Value>()
            .await
            .ok()?;
        json.pointer("/data/info/uname")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }

    /// 从 getRoomPlayInfo 流列表中选取可用地址
    /// 优先级：http_hls/ts(m3u8) > http_hls/fmp4(m3u8) > flv
    fn pick_stream_url(json: &serde_json::Value) -> Option<String> {
        let streams = json
            .pointer("/data/playurl_info/playurl/stream")
            .and_then(|v| v.as_array())?;

        // (协议, 格式, url) 三元组收集
        let mut hls_ts: Option<String> = None;
        let mut hls_other: Option<String> = None;
        let mut flv: Option<String> = None;

        for st in streams {
            let protocol = st.get("protocol_name").and_then(|v| v.as_str()).unwrap_or("");
            for f in st.get("format").and_then(|v| v.as_array()).into_iter().flatten() {
                let format = f.get("format_name").and_then(|v| v.as_str()).unwrap_or("");
                for c in f.get("codec").and_then(|v| v.as_array()).into_iter().flatten() {
                    let host = c.pointer("/url_info/0/host").and_then(|v| v.as_str());
                    let base = c.get("base_url").and_then(|v| v.as_str());
                    let extra = c.pointer("/url_info/0/extra").and_then(|v| v.as_str());
                    if let (Some(host), Some(base)) = (host, base) {
                        let url = format!("{}{}{}", host.trim_end_matches('/'), base, extra.unwrap_or(""));
                        match (protocol, format) {
                            ("http_hls", "ts") if hls_ts.is_none() => hls_ts = Some(url),
                            ("http_hls", _) if hls_other.is_none() => hls_other = Some(url),
                            (_, "flv") if flv.is_none() => flv = Some(url),
                            _ => {}
                        }
                    }
                }
            }
        }

        hls_ts.or(hls_other).or(flv)
    }

    async fn get_m3u8_stream_url(&self, room_id: &str) -> anyhow::Result<Option<String>> {
        let url = format!(
            "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo?room_id={}&protocol=0,1&format=0,1,2&codec=0,1&qn=10000&platform=web&ptype=8",
            room_id
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .header("Referer", REFERER)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;

        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("unknown");
            anyhow::bail!("getRoomPlayInfo 返回错误 {}: {}", code, msg);
        }

        Ok(Self::pick_stream_url(&json))
    }
}

impl Default for BilibiliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析热门列表条目为统一房间卡片（正在直播的房间）
fn parse_hot_item(item: &serde_json::Value) -> Option<RoomInfo> {
    let id = item.get("roomid").and_then(|v| v.as_u64())?.to_string();
    let title = item.get("title").and_then(|v| v.as_str())?.to_string();
    let username = item
        .get("uname")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let online = item.get("online").and_then(|v| v.as_u64()).unwrap_or(0);
    let cover = item
        .get("cover")
        .and_then(|v| v.as_str())
        .filter(|s| s.starts_with("http"))
        .map(|s| s.to_string());

    Some(RoomInfo {
        id,
        platform: "bilibili".to_string(),
        title,
        username,
        view_count: online,
        status: LiveStatus::Live,
        cover,
    })
}

#[async_trait]
impl crate::adapter::PlatformAdapter for BilibiliAdapter {
    fn name(&self) -> &'static str {
        "bilibili"
    }

    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        crate::adapter::PlatformCapability::ROOM_DETAIL
            | crate::adapter::PlatformCapability::LIVE_STATUS
            | crate::adapter::PlatformCapability::STREAM_URLS
    }

    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        // B 站无公开搜索接口（x/web-interface/search 返回 412 风控，需登录 cookie）。
        // 仅支持数字房间号（短号/长号均可，如 6 / 7734200）。
        if keyword.is_empty() || !keyword.chars().all(|c| c.is_ascii_digit()) {
            anyhow::bail!("B 站仅支持数字房间号直达（关键词搜索受平台风控限制）");
        }
        let info = self.room_detail(keyword).await?;
        Ok(vec![info])
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let json = self.fetch_room_info(room_id).await?;
        let data = json
            .get("data")
            .ok_or_else(|| anyhow::anyhow!("缺少 data 字段"))?;

        let real_room_id = data
            .get("room_id")
            .and_then(|v| v.as_u64())
            .map(|r| r.to_string())
            .unwrap_or_else(|| room_id.to_string());
        let title = data
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown Title")
            .to_string();
        let uid = data.get("uid").and_then(|v| v.as_u64()).unwrap_or(0);
        let username = self
            .fetch_uname(uid)
            .await
            .unwrap_or_else(|| format!("UID{}", uid));
        let live_status = data
            .get("live_status")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(RoomInfo {
            id: real_room_id,
            platform: "bilibili".to_string(),
            title,
            username,
            view_count: 0,
            status: if live_status == 1 {
                LiveStatus::Live
            } else {
                LiveStatus::Sleeping
            },
            cover: data
                .get("user_cover")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        })
    }

    async fn stream_urls(&self, room_id: &str) -> AdapterResult {
        match self.get_m3u8_stream_url(room_id).await {
            Ok(Some(url)) => Ok(StreamResult {
                platform: "bilibili".to_string(),
                room_id: room_id.to_string(),
                url: vec![url],
                quality: vec![],
            }),
            Ok(None) => Err(AdapterError::ExternalOnly {
                url: format!("https://live.bilibili.com/{}", room_id),
            }),
            Err(e) => {
                log::warn!("B 站流获取失败（{}），走外部浏览器兜底", e);
                Err(AdapterError::ExternalOnly {
                    url: format!("https://live.bilibili.com/{}", room_id),
                })
            }
        }
    }

    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus> {
        let json = self.fetch_room_info(room_id).await?;
        let live_status = json
            .pointer("/data/live_status")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(if live_status == 1 {
            LiveStatus::Live
        } else {
            LiveStatus::Sleeping
        })
    }

    /// B 站弹幕 WS 需要鉴权 token（danmaku_info 接口），暂未接入
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }

    /// 热门/推荐直播间（首页 getList 接口，实测无需登录）
    /// 合并推荐位 + 各分区模块，按 roomid 去重
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let url = "https://api.live.bilibili.com/xlive/web-interface/v1/index/getList?platform=web&page=1";
        let resp = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;

        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            let msg = json.get("message").and_then(|v| v.as_str()).unwrap_or("unknown");
            anyhow::bail!("getList 返回错误 {}: {}", code, msg);
        }

        let mut rooms: Vec<RoomInfo> = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // 推荐位（recommend_room_list：字段在顶层）
        if let Some(list) = json.pointer("/data/recommend_room_list").and_then(|v| v.as_array()) {
            for item in list {
                if let Some(room) = parse_hot_item(item) {
                    if seen.insert(room.id.clone()) {
                        rooms.push(room);
                    }
                }
            }
        }

        // 各分区模块（room_list[].list）
        if let Some(modules) = json.pointer("/data/room_list").and_then(|v| v.as_array()) {
            for module in modules {
                if let Some(list) = module.get("list").and_then(|v| v.as_array()) {
                    for item in list {
                        if let Some(room) = parse_hot_item(item) {
                            if seen.insert(room.id.clone()) {
                                rooms.push(room);
                            }
                        }
                    }
                }
            }
        }

        Ok(rooms)
    }
}
