//! Kuaishou 快手适配器
//!
//! 快手的房间页有风控（`/u/{id}` 返回风险页），但分类榜单接口游客可用：
//! - `live_api/category/data`：分类（游戏）列表，含房间数；
//! - `live_api/gameboard/list?gameId=...`：每个分类下的直播间列表，含
//!   `caption/poster/author/playUrls[].adaptationSet.representation[]`；
//!   `representation[].url` 为 FLV 直播流（https）。
//!
//! 因此房间详情/流地址均从分类榜单缓存中匹配获取；关键词搜索按分类名
//! （如「王者荣耀」「英雄联盟」）取对应榜单，或在榜单中过滤标题/主播名。

use crate::{AdapterError, AdapterResult, LiveStatus, QualityOption, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::RwLock;
use std::time::{Duration, Instant};

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const REFERER: &str = "https://live.kuaishou.com/";
const CATEGORY_URL: &str = "https://live.kuaishou.com/live_api/category/data";
const GAMEBOARD_URL: &str = "https://live.kuaishou.com/live_api/gameboard/list";
const PLATFORM: &str = "kuaishou";
const FOLLOW_ALL_URL: &str = "https://live.kuaishou.com/live_api/follow/all";

/// 榜单缓存有效期（榜单中的播放地址带时效签名，过期后需重新获取）
const CACHE_TTL: Duration = Duration::from_secs(30);
/// 分类列表缓存有效期
const CATEGORY_TTL: Duration = Duration::from_secs(600);
/// 无缓存时抓取的榜单数量（分类按房间数降序取前 N 个）
const BOARD_LIMIT: usize = 6;

#[derive(Clone)]
struct KsCategory {
    id: String,
    name: String,
    room_count: u64,
    /// 游戏分类（SYXX/WYJJ/DJRY）；综合分类（购物/音乐/其他）没有 gameboard
    is_game: bool,
}

/// 榜单条目（含播放地址，RoomInfo 不保存流地址，只能缓存在适配器内）
#[derive(Clone)]
struct KsRoom {
    id: String,
    author_id: String,
    title: String,
    username: String,
    view_count: u64,
    cover: Option<String>,
    started_at: Option<i64>,
    plays: Vec<QualityOption>,
}

impl KsRoom {
    fn to_room_info(&self) -> RoomInfo {
        RoomInfo {
            id: self.id.clone(),
            platform: PLATFORM.to_string(),
            title: self.title.clone(),
            username: self.username.clone(),
            view_count: self.view_count,
            status: if self.plays.is_empty() {
                LiveStatus::Sleeping
            } else {
                LiveStatus::Live
            },
            cover: crate::adapter::normalize_cover_url(self.cover.clone()),
            started_at: self.started_at,
        }
    }

    fn from_item(item: &Value) -> Option<Self> {
        let id = item.get("id")?.as_str()?.to_string();
        let author_id = item
            .pointer("/author/id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let plays = collect_plays(item);
        Some(Self {
            id,
            author_id,
            title: item
                .get("caption")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("快手直播")
                .to_string(),
            username: item
                .pointer("/author/name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or("未知主播")
                .to_string(),
            view_count: parse_count(item.get("watchingCount").unwrap_or(&Value::Null)),
            cover: item.get("poster").and_then(Value::as_str).map(str::to_string),
            // 快手字段拼写为 statrtTime，单位毫秒
            started_at: item
                .get("statrtTime")
                .and_then(Value::as_i64)
                .map(|ms| ms / 1000)
                .filter(|s| *s > 0),
            plays,
        })
    }
}

pub struct KuaishouAdapter {
    client: Client,
    catalog: RwLock<Option<(Instant, Vec<KsRoom>)>>,
    categories: RwLock<Option<(Instant, Vec<KsCategory>)>>,
    /// 平台登录 cookie（应用内登录后注入；携带后可访问关注列表等登录态接口）
    cookies: RwLock<Option<String>>,
}

impl KuaishouAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(12))
                // 快手对 HTTP/2 请求存在 TLS 指纹级风控（2026-11 实测 curl/1.1 可通而 reqwest h2 被拒），锁定 1.1
                .http1_only()
                .use_rustls_tls()
                .build()
                .unwrap(),
            catalog: RwLock::new(None),
            categories: RwLock::new(None),
            cookies: RwLock::new(None),
        }
    }

    async fn get_json(&self, url: &str, query: &[(&str, &str)]) -> anyhow::Result<Value> {
        let resp = self
            .client
            .get(url)
            .query(query)
            .header("User-Agent", USER_AGENT)
            .header("Referer", REFERER)
            // 快手对非浏览器头集存在指纹风控（2026-11 实测：缺 Accept/Accept-Language 的
            // 裸 reqwest 请求被拒，curl 同头可通），补齐浏览器级头集
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        log::debug!("[ks] {} -> {} len={} head={}", url, status, body.len(), &body[..body.len().min(100)]);
        if !status.is_success() {
            anyhow::bail!("HTTP {}: {}", status, &body[..body.len().min(80)]);
        }
        Ok(serde_json::from_str(&body).map_err(|e| anyhow::anyhow!("JSON 解析失败({} 字节): {}", body.len(), e))?)
    }

    async fn categories(&self) -> anyhow::Result<Vec<KsCategory>> {
        if let Some((at, cats)) = self.categories.read().unwrap().clone() {
            if at.elapsed() < CATEGORY_TTL {
                return Ok(cats);
            }
        }

        let json = self.get_json(CATEGORY_URL, &[]).await?;
        let mut cats = Vec::new();
        if let Some(list) = json.pointer("/data/list").and_then(Value::as_array) {
            for item in list {
                let id = match item.get("id").and_then(Value::as_str) {
                    Some(id) if !id.is_empty() => id.to_string(),
                    _ => continue,
                };
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                cats.push(KsCategory {
                    id,
                    name,
                    room_count: parse_count(item.get("roomCount").unwrap_or(&Value::Null)),
                    is_game: !matches!(
                        item.get("categoryAbbr").and_then(Value::as_str),
                        None | Some("") | Some("ZH")
                    ),
                });
            }
        }
        if cats.is_empty() {
            anyhow::bail!("快手分类列表为空");
        }
        cats.sort_by(|a, b| b.room_count.cmp(&a.room_count));
        // 综合分类（购物/音乐/其他）没有游戏榜单，优先使用游戏分类
        if cats.iter().any(|c| c.is_game) {
            cats.retain(|c| c.is_game);
        }

        *self.categories.write().unwrap() = Some((Instant::now(), cats.clone()));
        Ok(cats)
    }

    async fn fetch_board(&self, game_id: &str) -> anyhow::Result<Vec<KsRoom>> {
        let json = self
            .get_json(
                GAMEBOARD_URL,
                &[("gameId", game_id), ("page", "1"), ("pageSize", "30")],
            )
            .await?;
        Ok(json
            .pointer("/data/list")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(KsRoom::from_item).collect())
            .unwrap_or_default())
    }

    /// 分类榜单聚合缓存（搜索/房间详情/流地址共用）
    async fn catalog(&self, force: bool) -> anyhow::Result<Vec<KsRoom>> {
        if !force {
            if let Some((at, rooms)) = self.catalog.read().unwrap().clone() {
                if at.elapsed() < CACHE_TTL {
                    return Ok(rooms);
                }
            }
        }

        let categories = self.categories().await.unwrap_or_else(|e| {
            log::warn!("快手分类获取失败（{}），回退到默认游戏分类", e);
            vec![]
        });
        let mut board_ids: Vec<String> = categories
            .iter()
            .take(BOARD_LIMIT)
            .map(|c| c.id.clone())
            .collect();
        if board_ids.is_empty() {
            board_ids.push("1001".to_string());
        }

        // 快手风控：并发 gameboard 请求会被降级为空数据（2026-11-12 实测，
        // 并发 6 个全部返回 {"data":{"list":[]}}，串行正常），必须串行请求
        let mut rooms = Vec::new();
        let mut seen = HashSet::new();
        let mut errs: Vec<String> = Vec::new();
        for id in &board_ids {
            match self.fetch_board(id).await {
                Ok(list) => {
                    for room in list {
                        if seen.insert(room.id.clone()) {
                            rooms.push(room);
                        }
                    }
                }
                Err(e) => {
                    // 失效监测需要根因：聚合进最终错误
                    let msg = format!("{}", &e.to_string()[..e.to_string().len().min(80)]);
                    log::warn!("快手榜单获取失败：{}", msg);
                    errs.push(msg);
                }
            }
        }
        if rooms.is_empty() {
            anyhow::bail!("快手分类榜单暂时不可用（{}）", errs.join(" | "));
        }

        *self.catalog.write().unwrap() = Some((Instant::now(), rooms.clone()));
        Ok(rooms)
    }

    /// 在榜单中查找房间（room id 或主播 id）；未命中时强制刷新一次缓存
    async fn find_room(&self, id: &str) -> anyhow::Result<KsRoom> {
        for force in [false, true] {
            let rooms = self.catalog(force).await?;
            if let Some(room) = rooms
                .iter()
                .find(|r| r.id == id || (!r.author_id.is_empty() && r.author_id == id))
            {
                return Ok(room.clone());
            }
        }
        anyhow::bail!(
            "快手榜单中未找到房间 {}（快手仅能通过分类榜单获取房间信息，可直接输入榜单中的房间号）",
            id
        )
    }

    /// 弹幕模块用：由直播间 id 反查主播 principalId（快手的 Referer 需要主播页路径）
    pub(crate) async fn lookup_principal(&self, room_id: &str) -> Option<String> {
        self.find_room(room_id)
            .await
            .ok()
            .map(|room| room.author_id)
            .filter(|id| !id.is_empty())
    }
}

impl Default for KuaishouAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_https(url: &str) -> String {
    url.strip_prefix("http://")
        .map(|rest| format!("https://{rest}"))
        .unwrap_or_else(|| url.to_string())
}

/// 观看人数/房间数：数字或「1.2万」「3亿」字符串
fn parse_count(v: &Value) -> u64 {
    match v {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => {
            let s = s.trim();
            if let Some(x) = s.strip_suffix('万') {
                x.parse::<f64>().ok().map(|n| (n * 10_000.0) as u64)
            } else if let Some(x) = s.strip_suffix('亿') {
                x.parse::<f64>().ok().map(|n| (n * 100_000_000.0) as u64)
            } else {
                s.parse().ok()
            }
            .unwrap_or(0)
        }
        _ => 0,
    }
}

/// 取第一个 playUrls 的全部码率，默认清晰度优先，其余按码率降序
fn collect_plays(item: &Value) -> Vec<QualityOption> {
    let play = match item
        .get("playUrls")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        Some(p) => p,
        None => return vec![],
    };
    let reps = match play
        .pointer("/adaptationSet/representation")
        .and_then(Value::as_array)
    {
        Some(r) => r,
        None => return vec![],
    };

    let mut plays: Vec<(bool, i64, QualityOption)> = reps
        .iter()
        .filter_map(|rep| {
            let url = rep.get("url").and_then(Value::as_str).filter(|u| !u.is_empty())?;
            Some((
                rep.get("defaultSelect").and_then(Value::as_bool).unwrap_or(false),
                rep.get("bitrate").and_then(Value::as_i64).unwrap_or(0),
                QualityOption {
                    label: rep
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("直播")
                        .to_string(),
                    url: normalize_https(url),
                },
            ))
        })
        .collect();
    plays.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    plays.into_iter().map(|(_, _, q)| q).collect()
}

#[async_trait]
impl crate::adapter::PlatformAdapter for KuaishouAdapter {
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
        let lower = kw.to_lowercase();

        // 1) 房间号/主播 ID 直达（榜单缓存命中）
        if let Some((_, rooms)) = self.catalog.read().unwrap().clone() {
            let hit: Vec<RoomInfo> = rooms
                .iter()
                .filter(|r| r.id == kw || (!r.author_id.is_empty() && r.author_id == kw))
                .map(KsRoom::to_room_info)
                .collect();
            if !hit.is_empty() {
                return Ok(hit);
            }
        }

        // 2) 分类名匹配：如「王者荣耀」「英雄联盟」「和平精英」
        let categories = self.categories().await.unwrap_or_default();
        let matched: Vec<&KsCategory> = categories
            .iter()
            .filter(|c| c.name.contains(kw) || (c.name.chars().count() >= 2 && kw.contains(&c.name)))
            .take(2)
            .collect();
        if !matched.is_empty() {
            let boards = futures::future::join_all(
                matched.iter().map(|c| self.fetch_board(&c.id)),
            )
            .await;
            let mut rooms = Vec::new();
            let mut seen = HashSet::new();
            for board in boards {
                if let Ok(list) = board {
                    for room in list {
                        if seen.insert(room.id.clone()) {
                            rooms.push(room.to_room_info());
                        }
                    }
                }
            }
            if !rooms.is_empty() {
                rooms.truncate(30);
                return Ok(rooms);
            }
        }

        // 3) 榜单标题/主播名过滤
        let rooms = self.catalog(false).await?;
        Ok(rooms
            .iter()
            .filter(|r| {
                r.title.to_lowercase().contains(&lower)
                    || r.username.to_lowercase().contains(&lower)
                    || r.id.eq_ignore_ascii_case(kw)
                    || (!r.author_id.is_empty() && r.author_id.eq_ignore_ascii_case(kw))
            })
            .map(KsRoom::to_room_info)
            .collect())
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let room = self.find_room(room_id).await?;
        Ok(room.to_room_info())
    }

    async fn stream_urls(&self, room_id: &str) -> AdapterResult {
        let room = match self.find_room(room_id).await {
            Ok(room) => room,
            Err(e) => {
                log::warn!("快手房间 {} 获取失败：{}", room_id, e);
                return Err(AdapterError::ExternalOnly {
                    url: format!("https://live.kuaishou.com/u/{}", room_id),
                });
            }
        };
        if room.plays.is_empty() {
            return Err(AdapterError::ExternalOnly {
                url: format!(
                    "https://live.kuaishou.com/u/{}",
                    if room.author_id.is_empty() {
                        &room.id
                    } else {
                        &room.author_id
                    }
                ),
            });
        }
        Ok(StreamResult {
            platform: PLATFORM.to_string(),
            room_id: room.id.clone(),
            url: room.plays.iter().map(|q| q.url.clone()).collect(),
            quality: room.plays,
        })
    }

    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus> {
        let room = self.find_room(room_id).await?;
        Ok(if room.plays.is_empty() {
            LiveStatus::Sleeping
        } else {
            LiveStatus::Live
        })
    }

    /// 热门直播间：分类榜单聚合去重
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let rooms = self.catalog(false).await?;
        Ok(rooms.iter().take(20).map(KsRoom::to_room_info).collect())
    }

    /// 快手弹幕协议需要 WebSocket 签名，暂未接入
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }

    /// 关注列表（live_api/follow/all，需登录 cookie）。
    /// 返回全部关注主播（含未开播），按 pcursor 游标分页。
    /// 注意：未登录时该接口返回空列表而非报错，因此登录态在本地先行校验。
    async fn follows(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let cookie = self
            .cookies
            .read()
            .unwrap()
            .clone()
            .filter(|c| !c.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("未登录快手，请先在设置中心完成扫码登录")
            })?;

        let mut out = Vec::new();
        let mut pcursor = String::new();
        for _page in 0..10 {
            let json: Value = self
                .client
                .get(FOLLOW_ALL_URL)
                .query(&[("pcursor", pcursor.as_str())])
                .header("User-Agent", USER_AGENT)
                .header("Referer", "https://live.kuaishou.com/my/follow")
                .header("Cookie", &cookie)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let data = json
                .get("data")
                .ok_or_else(|| anyhow::anyhow!("快手关注列表返回异常"))?;
            let list = data
                .get("list")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for item in &list {
                let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("未知主播")
                    .to_string();
                let living = item
                    .get("living")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                out.push(RoomInfo {
                    id: id.to_string(),
                    platform: PLATFORM.to_string(),
                    // 关注列表没有直播标题，用主播名代替
                    title: name.clone(),
                    username: name,
                    view_count: 0,
                    status: if living {
                        LiveStatus::Live
                    } else {
                        LiveStatus::Sleeping
                    },
                    cover: crate::adapter::normalize_cover_url(
                        item.get("avatar")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    ),
                    started_at: None,
                });
            }
            pcursor = data
                .get("pcursor")
                .and_then(Value::as_str)
                .unwrap_or("no_more")
                .to_string();
            if pcursor.is_empty() || pcursor == "no_more" {
                break;
            }
        }
        Ok(out)
    }

    fn set_cookies(&self, cookie: Option<String>) {
        *self.cookies.write().unwrap() = cookie.filter(|c| !c.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::PlatformAdapter;

    /// 网络集成测试：热门榜单应返回可播放房间
    #[tokio::test]
    #[ignore]
    async fn hot_rooms_return_playable() {
        let adapter = KuaishouAdapter::new();
        // 先分别打印分类/榜单结果，便于定位接口风控
        match adapter.categories().await {
            Ok(cats) => println!("分类数：{}，前3：{:?}", cats.len(), cats.iter().take(3).map(|c| (&c.id, &c.name)).collect::<Vec<_>>()),
            Err(e) => println!("分类获取失败：{e:#}"),
        }
        match adapter.fetch_board("1001").await {
            Ok(list) => println!("榜单条目：{}", list.len()),
            Err(e) => println!("榜单获取失败：{e:#}"),
        }
        let rooms = adapter.hot_rooms().await.expect("快手热门失败");
        assert!(!rooms.is_empty(), "快手热门不应为空");
        println!(
            "前3条：{:?}",
            rooms
                .iter()
                .take(3)
                .map(|r| (r.id.clone(), r.username.clone(), r.status.clone()))
                .collect::<Vec<_>>()
        );
        let first = rooms.first().unwrap();
        let stream = adapter.stream_urls(&first.id).await.expect("快手流获取失败");
        assert!(!stream.url.is_empty());
        println!("FLV: {}", stream.url[0]);

        let search = adapter.search("和平精英").await.expect("快手分类搜索失败");
        println!("搜索「和平精英」：{} 条", search.len());
        assert!(!search.is_empty(), "分类搜索应有结果");
    }
}
