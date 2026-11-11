//! Huya 虎牙适配器
//! 通过房间页内嵌 JSON 提取直播流（原生干净流，无广告）
//!
//! 说明：交接文档中的 dynamicupscbk.huya.com 端点与 search.cdn.huya.com
//! 经实测均不存在/已下线，改为抓取 https://www.huya.com/{room_id} 页面，
//! 从内嵌 JSON（gameLiveInfo / gameStreamInfoList）中提取数据，
//! 流地址使用 reverse::huya_signature 重算 wsSecret 后拼接 m3u8。

use crate::reverse::huya_signature::build_stream_url;
use crate::{AdapterError, AdapterResult, LiveStatus, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub struct HuyaAdapter {
    client: Client,
    /// 平台登录 cookie（应用内登录后注入；携带后可访问需登录态接口）
    cookies: std::sync::RwLock<Option<String>>,
}

/// 房间页内嵌 JSON 中提取出的流条目
struct StreamEntry {
    hls_url: String,
    hls_suffix: String,
    hls_anti_code: String,
    stream_name: String,
    presenter_uid: i64,
}

/// 从 page[start..] 查找 "key":"value"（容忍空格），返回 JSON 字符串值
fn find_json_str(text: &str, key: &str, start: usize) -> Option<String> {
    let hay = text.get(start..)?;
    for pat in [format!("\"{}\":\"", key), format!("\"{}\": \"", key)] {
        if let Some(i) = hay.find(&pat) {
            let rest = &hay[i + pat.len()..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].replace("\\\"", "\""));
            }
        }
    }
    None
}

/// 从 page[start..] 查找 "key":数字
fn find_json_num(text: &str, key: &str, start: usize) -> Option<i64> {
    let hay = text.get(start..)?;
    let pat = format!("\"{}\":", key);
    let i = hay.find(&pat)?;
    let rest = hay[i + pat.len()..].trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '-')
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// JSON 值宽容读取：数字或字符串统一为 String（虎牙接口同字段类型不稳定）
fn json_as_string_or_num(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// HTML 文本反转义（订阅页标题/封面 URL 中的实体）
fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// 去掉 HTML 标签，保留纯文本
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

impl HuyaAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            cookies: std::sync::RwLock::new(None),
        }
    }

    /// 为请求附加登录 cookie（未登录时不加）
    fn authed(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.cookies.read().unwrap().clone() {
            Some(c) if !c.is_empty() => rb.header("Cookie", c),
            _ => rb,
        }
    }

    async fn fetch_room_page(&self, room_id: &str) -> anyhow::Result<String> {
        let url = format!("https://www.huya.com/{}", room_id);
        let resp = self
            .authed(self.client.get(&url))
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        let text = resp.text().await?;
        Ok(text)
    }

    /// 从房间页 HTML 中提取第一个流条目（列表为空表示未开播）
    fn extract_stream_entry(text: &str) -> Option<StreamEntry> {
        let list_idx = text.find("\"gameStreamInfoList\":")?;
        let after = text[list_idx + "\"gameStreamInfoList\":".len()..].trim_start();
        // 空数组 → 未开播
        let inner = after.strip_prefix('[')?.trim_start();
        if inner.starts_with(']') {
            return None;
        }

        let hls_url = find_json_str(text, "sHlsUrl", list_idx)?;
        let hls_suffix = find_json_str(text, "sHlsUrlSuffix", list_idx)?;
        let hls_anti_code = find_json_str(text, "sHlsAntiCode", list_idx)?;
        let stream_name = find_json_str(text, "sStreamName", list_idx)?;
        let presenter_uid = find_json_num(text, "lPresenterUid", list_idx)?;

        // 实测对所有 liveSourceType（含 12/imgplus）重算签名均有效，始终重算
        Some(StreamEntry {
            hls_url,
            hls_suffix,
            hls_anti_code,
            stream_name,
            presenter_uid,
        })
    }

    /// 拼接可用的 m3u8 地址
    async fn get_m3u8_stream_url(&self, room_id: &str) -> Option<String> {
        let text = self.fetch_room_page(room_id).await.ok()?;
        let entry = Self::extract_stream_entry(&text)?;
        build_stream_url(
            &entry.hls_url,
            &entry.hls_suffix,
            &entry.stream_name,
            &entry.hls_anti_code,
            entry.presenter_uid,
            true,
        )
    }

    /// 搜索接口返回的 docs 条目 → 兜底 RoomInfo（状态按索引标记）
    fn room_info_from_search_doc(room_id: &str, d: &serde_json::Value) -> RoomInfo {
        RoomInfo {
            id: room_id.to_string(),
            platform: "huya".to_string(),
            title: d["live_intro"].as_str().unwrap_or("Unknown").to_string(),
            username: d["game_nick"].as_str().unwrap_or("Unknown").to_string(),
            view_count: d["game_activityCount"].as_i64().unwrap_or(0).max(0) as u64,
            status: if d["gameLiveOn"].as_bool().unwrap_or(false) {
                LiveStatus::Live
            } else {
                LiveStatus::Sleeping
            },
            cover: None,
            started_at: json_as_string_or_num(&d["rec_live_time"]).and_then(|s| s.parse().ok()),
        }
    }
}

impl Default for HuyaAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::adapter::PlatformAdapter for HuyaAdapter {
    fn name(&self) -> &'static str {
        "huya"
    }

    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        crate::adapter::PlatformCapability::SEARCH
            | crate::adapter::PlatformCapability::ROOM_DETAIL
            | crate::adapter::PlatformCapability::STREAM_URLS
            | crate::adapter::PlatformCapability::LIVE_STATUS
    }

    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        let kw = keyword.trim();
        if kw.is_empty() {
            return Ok(vec![]);
        }

        // 1) 房间号/别名直达（如 988、lpl），命中即返回唯一结果
        if let Ok(info) = self.room_detail(kw).await {
            return Ok(vec![info]);
        }

        // 2) 关键词搜索：search.cdn.huya.com 需 v=4 参数（v=1 返回 not supported function）
        let url = format!(
            "https://search.cdn.huya.com/?m=Search&do=getSearchContent&q={}&uid=0&v=4&typ=-5&livestate=0&rows=20",
            urlencoding::encode(kw)
        );
        let resp = self
            .authed(self.client.get(&url))
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://www.huya.com/")
            .send()
            .await?
            .error_for_status()?;
        let json: serde_json::Value = resp.json().await?;
        let docs = json["response"]["1"]["docs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if docs.is_empty() {
            log::info!("虎牙关键词 {} 无搜索结果", kw);
            return Ok(vec![]);
        }

        // 搜索索引的 gameLiveOn 长期滞后（实测恒为 false），状态不可信；
        // 对前 SEARCH_VERIFY_COUNT 条并发请求房间页校验真实状态/封面/人气。
        const SEARCH_VERIFY_COUNT: usize = 8;
        let verify_ids: Vec<String> = docs
            .iter()
            .take(SEARCH_VERIFY_COUNT)
            .filter_map(|d| d["room_id"].as_i64().map(|id| id.to_string()))
            .collect();
        let details =
            futures::future::join_all(verify_ids.iter().map(|id| self.room_detail(id))).await;

        let mut results: Vec<RoomInfo> = Vec::new();
        let mut verify_iter = details.into_iter();
        for d in docs.iter() {
            let room_id = match d["room_id"].as_i64() {
                Some(id) => id.to_string(),
                None => continue,
            };
            // 前 SEARCH_VERIFY_COUNT 条逐个消费校验结果（顺序与 verify_ids 一致）
            if let Some(res) = verify_iter.next() {
                match res {
                    Ok(info) => results.push(info),
                    Err(e) => {
                        log::warn!("虎牙搜索结果 {} 校验失败：{}，使用索引数据", room_id, e);
                        results.push(Self::room_info_from_search_doc(&room_id, d));
                    }
                }
            } else {
                results.push(Self::room_info_from_search_doc(&room_id, d));
            }
        }
        Ok(results)
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let text = self.fetch_room_page(room_id).await?;
        let live_idx = text
            .find("\"gameLiveInfo\":")
            .ok_or_else(|| anyhow::anyhow!("页面缺少 gameLiveInfo"))?;

        let title = find_json_str(&text, "roomName", live_idx)
            .or_else(|| find_json_str(&text, "introduction", live_idx))
            .unwrap_or_else(|| "Unknown Title".to_string());
        let username = find_json_str(&text, "nick", live_idx)
            .unwrap_or_else(|| "Unknown Streamer".to_string());
        let view_count = find_json_num(&text, "totalCount", live_idx).unwrap_or(0) as u64;
        let is_live = Self::extract_stream_entry(&text).is_some();

        Ok(RoomInfo {
            id: room_id.to_string(),
            platform: "huya".to_string(),
            title,
            username,
            view_count,
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Sleeping
            },
            cover: crate::adapter::normalize_cover_url(
                find_json_str(&text, "screenshot", live_idx).filter(|s| s.starts_with("http")),
            ),
            started_at: None,
        })
    }

    async fn stream_urls(&self, room_id: &str) -> AdapterResult {
        match self.get_m3u8_stream_url(room_id).await {
            Some(url) => Ok(StreamResult {
                platform: "huya".to_string(),
                room_id: room_id.to_string(),
                url: vec![url],
                quality: vec![],
            }),
            None => Err(AdapterError::ExternalOnly {
                url: format!("https://www.huya.com/{}", room_id),
            }),
        }
    }

    /// 订阅列表（i.huya.com 订阅页服务端渲染，需登录 cookie）
    /// 页面内 <li data-link="https://www.huya.com/{room}"> 即一条订阅，直接解析 HTML，
    /// 不再额外请求房间页；返回含未开播主播。
    async fn follows(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let cookie = self
            .cookies
            .read()
            .unwrap()
            .clone()
            .filter(|c| !c.is_empty())
            .ok_or_else(|| anyhow::anyhow!("未登录虎牙，请先在设置中心完成内嵌登录"))?;

        let resp = self
            .client
            .get("https://i.huya.com/index.php?m=Subscribe")
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://i.huya.com/")
            .header("Cookie", &cookie)
            .send()
            .await?
            .error_for_status()?;
        let html = resp.text().await?;

        let Some(start) = html.find("id=\"subWrap\"") else {
            // 订阅页结构变化或未登录，返回空而不是报错
            return Ok(vec![]);
        };
        let Some(end_rel) = html[start..].find("</ul>") else {
            return Ok(vec![]);
        };
        let block = &html[start..start + end_rel];

        let li_re =
            regex::Regex::new(r#"(?s)<li[^>]*data-link="([^"]+)"[^>]*>(.*?)</li>"#).unwrap();
        let p1_re = regex::Regex::new(r#"(?s)<p class="p_1">(.*?)</p>"#).unwrap();
        let p2_re = regex::Regex::new(r#"(?s)<p class="p_2">(.*?)</p>"#).unwrap();
        let img_re = regex::Regex::new(r#"(?s)<img[^>]+src="([^"]+)""#).unwrap();

        let mut rooms: Vec<RoomInfo> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for cap in li_re.captures_iter(block) {
            let link = html_unescape(&cap[1]);
            let Some(id) = link
                .split('?')
                .next()
                .unwrap_or(&link)
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(String::from)
            else {
                continue;
            };
            if !seen.insert(id.clone()) {
                continue;
            }
            let body = &cap[2];
            let username = p1_re
                .captures(body)
                .map(|c| strip_tags(&html_unescape(&c[1])))
                .unwrap_or_default();
            let title = p2_re
                .captures(body)
                .map(|c| strip_tags(&html_unescape(&c[1])))
                .unwrap_or_default();
            let cover = img_re
                .captures(body)
                .map(|c| html_unescape(&c[1]))
                .filter(|s| s.starts_with("http"));
            let live = body.contains("class=\"live_icon\"");
            rooms.push(RoomInfo {
                id,
                platform: "huya".to_string(),
                title: if title.is_empty() {
                    format!("{} 的直播间", username)
                } else {
                    title
                },
                username,
                view_count: 0,
                status: if live {
                    LiveStatus::Live
                } else {
                    LiveStatus::Sleeping
                },
                cover: crate::adapter::normalize_cover_url(cover),
                started_at: None,
            });
        }
        Ok(rooms)
    }

    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus> {
        let text = self.fetch_room_page(room_id).await?;
        let is_live = Self::extract_stream_entry(&text).is_some();

        Ok(if is_live {
            LiveStatus::Live
        } else {
            LiveStatus::Sleeping
        })
    }

    /// 热门直播间（cache.php getLiveListByPage，实测无需登录）
    /// 取首页前 12 条；roomName 可能为空，回退 introduction → gameFullName
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let url = "https://www.huya.com/cache.php?m=LiveList&do=getLiveListByPage&tagAll=0&page=1";
        let resp = self
            .authed(self.client.get(url))
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://www.huya.com/")
            .send()
            .await?
            .error_for_status()?;
        let json: serde_json::Value = resp.json().await?;

        let mut rooms = Vec::new();
        if let Some(items) = json.pointer("/data/datas").and_then(|v| v.as_array()) {
            for item in items.iter().take(12) {
                // profileRoom/totalCount 可能是数字也可能是字符串，宽容解析
                let id = match json_as_string_or_num(&item["profileRoom"]) {
                    Some(id) if !id.is_empty() => id,
                    _ => continue,
                };
                let title = item["roomName"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .or_else(|| item["introduction"].as_str())
                    .or_else(|| item["gameFullName"].as_str())
                    .unwrap_or("Huya Live")
                    .to_string();
                let view_count = json_as_string_or_num(&item["totalCount"])
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                rooms.push(RoomInfo {
                    id,
                    platform: "huya".to_string(),
                    title,
                    username: item["nick"].as_str().unwrap_or("Unknown").to_string(),
                    view_count,
                    status: LiveStatus::Live,
                    cover: crate::adapter::normalize_cover_url(
                        item["screenshot"]
                            .as_str()
                            .filter(|s| s.starts_with("http"))
                            .map(String::from),
                    ),
                    started_at: None,
                });
            }
        }
        Ok(rooms)
    }

    /// 虎牙弹幕为 TC 二进制协议，暂未接入，返回 None（前端按无弹幕处理）
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }

    fn set_cookies(&self, cookie: Option<String>) {
        *self.cookies.write().unwrap() = cookie;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::PlatformAdapter;

    /// 网络集成测试：关键词搜索应返回含目标主播的结果
    #[tokio::test]
    #[ignore] // 需要真实网络，默认跳过；cargo test -- --ignored 验证
    async fn search_by_keyword_finds_streamer() {
        let adapter = HuyaAdapter::new();
        let results = adapter.search("悲喜").await.expect("搜索失败");
        assert!(!results.is_empty(), "关键词搜索应返回非空结果");
        let hit = results
            .iter()
            .find(|r| r.username.contains("悲喜") && r.id == "988");
        assert!(hit.is_some(), "应能搜到房间 988（RASH悲喜）");
        println!(
            "前3条结果：{:?}",
            results
                .iter()
                .take(3)
                .map(|r| (r.id.clone(), r.username.clone(), r.status.clone()))
                .collect::<Vec<_>>()
        );
    }

    /// 房间号直达仍应可用
    #[tokio::test]
    #[ignore]
    async fn search_by_room_id_still_works() {
        let adapter = HuyaAdapter::new();
        let results = adapter.search("988").await.expect("搜索失败");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "988");
    }
}
