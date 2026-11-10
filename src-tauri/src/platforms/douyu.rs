//! Douyu 斗鱼适配器
//! 房间信息使用 betard 接口（实测可用）；
//! 流地址通过 swf_api 取签名密钥，按 reverse::douyu_signature 计算 sign，
//! POST lapi/live/getH5Play 获取 flv 直播流（实测 error=0）；
//! 支持别名房间号（如 pigff）：抓房间页解析 window.room_id 得到数字 ID。

use crate::reverse::douyu_signature;
use crate::{AdapterError, AdapterResult, LiveStatus, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

pub struct DouyuAdapter {
    client: Client,
    /// 别名 → 数字房间号 缓存，避免每次搜索重复抓页面
    alias_cache: Mutex<HashMap<String, String>>,
}

impl DouyuAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            alias_cache: Mutex::new(HashMap::new()),
        }
    }

    /// 别名/数字统一解析为数字房间号（数字直接返回，别名抓页面解析并缓存）
    async fn resolve_room_id(&self, room_id: &str) -> anyhow::Result<String> {
        if room_id.chars().all(|c| c.is_ascii_digit()) {
            return Ok(room_id.to_string());
        }

        if let Ok(cache) = self.alias_cache.lock() {
            if let Some(hit) = cache.get(room_id) {
                return Ok(hit.clone());
            }
        }

        let html = self.fetch_room_page(room_id).await?;
        let numeric = extract_numeric_room_id(&html)
            .ok_or_else(|| anyhow::anyhow!("别名 {} 未解析到数字房间号（房间可能不存在）", room_id))?;

        if let Ok(mut cache) = self.alias_cache.lock() {
            cache.insert(room_id.to_string(), numeric.clone());
        }
        log::info!("斗鱼别名 {} → 房间号 {}", room_id, numeric);
        Ok(numeric)
    }

    /// 抓取房间页 HTML（用于别名解析）
    async fn fetch_room_page(&self, slug: &str) -> anyhow::Result<String> {
        let url = format!("https://www.douyu.com/{}", slug);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        Ok(resp.text().await?)
    }

    /// 房间信息（真实接口：含房间名/主播名/开播状态）
    async fn fetch_betard(&self, room_id: &str) -> anyhow::Result<serde_json::Value> {
        let url = format!("https://www.douyu.com/betard/{}/", room_id);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;
        Ok(json)
    }

    fn parse_room_info(&self, room_id: &str, json: &serde_json::Value) -> RoomInfo {
        // betard 不同房间类型结构不一致（room_name/nickname 可能顶层或嵌套），深度查找
        let title = find_field_value(json, "room_name")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Douyu Room".to_string());
        let username = find_field_value(json, "nickname")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Unknown Streamer".to_string());
        // showStatus 为字符串 "1" 或数字 1（直播中）
        let is_live = find_field_value(json, "showStatus").as_deref() == Some("1");

        RoomInfo {
            id: room_id.to_string(),
            platform: "douyu".to_string(),
            title,
            username,
            view_count: 0,
            status: if is_live {
                LiveStatus::Live
            } else {
                LiveStatus::Sleeping
            },
        }
    }

    /// 从 swf_api 返回的混淆 JS 中提取签名密钥（vdwdae325w_64we）
    async fn fetch_sign_key(&self, room_id: &str) -> anyhow::Result<String> {
        let url = format!("https://www.douyu.com/swf_api/homeH5Enc?rids={}", room_id);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;

        let js = json
            .get("data")
            .and_then(|d| d.get(&format!("room{}", room_id)))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("swf_api 响应缺少签名脚本"))?;

        extract_js_string(js, "vdwdae325w_64we")
            .ok_or_else(|| anyhow::anyhow!("签名脚本中未找到密钥 vdwdae325w_64we"))
    }

    /// 获取 flv 直播流地址（签名与 swf_api 均要求数字房间号）
    async fn get_flv_stream_url(&self, room_id: &str) -> anyhow::Result<String> {
        let numeric_id = self.resolve_room_id(room_id).await?;
        let key = self.fetch_sign_key(&numeric_id).await?;
        let uuid = uuid::Uuid::new_v4().simple().to_string();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let sign_query = douyu_signature::sign(&numeric_id, &uuid, &ts, &key);

        // sign_query 形如 "v=...&did=...&tt=...&sign=..."，拆成表单键值
        let mut form: Vec<(&str, &str)> = vec![("rate", "0")];
        for pair in sign_query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                form.push((k, v));
            }
        }

        let url = format!("https://www.douyu.com/lapi/live/getH5Play/{}", numeric_id);
        let resp = self
            .client
            .post(&url)
            .header("User-Agent", USER_AGENT)
            .header("Referer", format!("https://www.douyu.com/{}", room_id))
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;

        let error = json.get("error").and_then(|v| v.as_i64()).unwrap_or(-1);
        if error != 0 {
            let msg = json.get("msg").and_then(|v| v.as_str()).unwrap_or("unknown");
            anyhow::bail!("getH5Play 返回错误 {}: {}", error, msg);
        }

        let rtmp_url = json
            .get("data")
            .and_then(|d| d.get("rtmp_url"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("响应缺少 rtmp_url"))?;
        let rtmp_live = json
            .get("data")
            .and_then(|d| d.get("rtmp_live"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("响应缺少 rtmp_live"))?;

        Ok(format!("{}/{}", rtmp_url.trim_end_matches('/'), rtmp_live))
    }
}

/// 从 JS 源码中提取 `var NAME = "VALUE";` 的字符串值
fn extract_js_string(js: &str, name: &str) -> Option<String> {
    let idx = js.find(name)?;
    let rest = &js[idx + name.len()..];
    let q1 = rest.find('"')?;
    let rest2 = &rest[q1 + 1..];
    let q2 = rest2.find('"')?;
    Some(rest2[..q2].to_string())
}

/// 深度优先查找字段值（字符串/数字），先查当前层级再递归子节点。
/// 斗鱼 betard 对不同房间类型返回的 JSON 结构不一致，字段可能在嵌套对象中。
fn find_field_value(value: &serde_json::Value, key: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(obj) => {
            if let Some(v) = obj.get(key) {
                match v {
                    serde_json::Value::String(s) if !s.is_empty() => return Some(s.clone()),
                    serde_json::Value::Number(n) => return Some(n.to_string()),
                    _ => {}
                }
            }
            for v in obj.values() {
                if let Some(found) = find_field_value(v, key) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => arr.iter().find_map(|v| find_field_value(v, key)),
        _ => None,
    }
}

/// 从斗鱼房间页 HTML 中解析真实数字房间号
/// 优先 window.room_id = 24422; 兜底 RSC payload 的 "room_id":24422
fn extract_numeric_room_id(html: &str) -> Option<String> {
    if let Some(i) = html.find("window.room_id") {
        let rest = html[i + "window.room_id".len()..].trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            let end = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            if end > 0 {
                return Some(rest[..end].to_string());
            }
        }
    }
    if let Some(i) = html.find("\"room_id\":") {
        let rest = &html[i + "\"room_id\":".len()..];
        let end = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if end > 0 {
            return Some(rest[..end].to_string());
        }
    }
    None
}

impl Default for DouyuAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "依赖真实网络：调试别名解析链路"]
    async fn debug_search_pigff() {
        let adapter = DouyuAdapter::new();
        let numeric = adapter.resolve_room_id("pigff").await.expect("解析失败");
        eprintln!("resolved id: {}", numeric);
        let json = adapter.fetch_betard(&numeric).await.expect("betard 失败");
        let text = serde_json::to_string(&json).expect("序列化失败");
        eprintln!("betard LEN: {}", text.len());
        eprintln!("betard HEAD: {}", &text[..text.len().min(300)]);
        eprintln!("room_name: {:?}", json.get("room_name"));
        eprintln!("nickname: {:?}", json.get("nickname"));
        eprintln!("showStatus: {:?}", json.get("showStatus"));
        let info = adapter.parse_room_info(&numeric, &json);
        eprintln!("title: {:?}", info.title);
    }

    #[tokio::test]
    #[ignore = "依赖真实网络：调试 betard 响应结构"]
    async fn debug_betard_24422() {
        let adapter = DouyuAdapter::new();
        let json = adapter.fetch_betard("24422").await.expect("betard 请求失败");
        let text = serde_json::to_string(&json).expect("序列化失败");
        println!("LEN: {}", text.len());
        println!("HEAD: {}", &text[..text.len().min(400)]);
        println!("has room_name: {}", text.contains("room_name"));
        println!("has nickname: {}", text.contains("nickname"));
    }
}

#[async_trait]
impl crate::adapter::PlatformAdapter for DouyuAdapter {
    fn name(&self) -> &'static str {
        "douyu"
    }

    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        crate::adapter::PlatformCapability::ROOM_DETAIL
            | crate::adapter::PlatformCapability::LIVE_STATUS
            | crate::adapter::PlatformCapability::STREAM_URLS
            | crate::adapter::PlatformCapability::EXTERNAL_ONLY
    }

    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        // 平台搜索接口已下线；改为房间号/别名直达解析：
        // 关键词 → 数字房间号（别名抓页面解析）→ betard 取真实房间信息
        match self.resolve_room_id(keyword).await {
            Ok(numeric_id) => match self.fetch_betard(&numeric_id).await {
                Ok(json) => {
                    let mut info = self.parse_room_info(&numeric_id, &json);
                    info.id = numeric_id;
                    Ok(vec![info])
                }
                Err(e) => {
                    log::warn!("斗鱼房间 {} 信息获取失败（{}）", numeric_id, e);
                    Ok(vec![])
                }
            },
            Err(e) => {
                log::info!("斗鱼关键词 {} 无法解析为房间：{}", keyword, e);
                Ok(vec![])
            }
        }
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let numeric_id = self.resolve_room_id(room_id).await?;
        let json = self.fetch_betard(&numeric_id).await?;
        let mut info = self.parse_room_info(&numeric_id, &json);
        info.id = numeric_id;
        Ok(info)
    }

    async fn stream_urls(&self, room_id: &str) -> AdapterResult {
        match self.get_flv_stream_url(room_id).await {
            Ok(url) => Ok(StreamResult {
                platform: "douyu".to_string(),
                room_id: room_id.to_string(),
                url: vec![url],
                quality: vec![],
            }),
            Err(e) => {
                log::warn!("斗鱼流获取失败（{}），走外部浏览器兜底", e);
                Err(AdapterError::ExternalOnly {
                    url: format!("https://www.douyu.com/{}", room_id),
                })
            }
        }
    }

    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus> {
        let numeric_id = self.resolve_room_id(room_id).await?;
        let json = self.fetch_betard(&numeric_id).await?;
        let info = self.parse_room_info(&numeric_id, &json);
        Ok(info.status)
    }

    /// 斗鱼弹幕协议未接入，返回 None（前端按无弹幕处理）
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }
}
