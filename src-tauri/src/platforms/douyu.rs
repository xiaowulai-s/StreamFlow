//! Douyu 斗鱼适配器
//! 房间信息使用 betard 接口（实测可用）；
//! 流地址通过 swf_api 取当日混淆签名 JS，经系统 JS 引擎（node/cscript）执行得到签名，
//! POST lapi/live/getH5Play 获取 flv 直播流（实测 error=0）。
//! 注意：签名混淆参数每日轮换，纯 Rust 硬编码公式次日即失效（已实测），故必须动态执行 JS。

use crate::{AdapterError, AdapterResult, LiveStatus, QualityOption, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const MOBILE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1";

pub struct DouyuAdapter {
    client: Client,
    /// 别名 → 数字房间号 缓存，避免每次搜索重复抓页面
    alias_cache: Mutex<HashMap<String, String>>,
    /// 平台登录 cookie（应用内登录后注入）
    cookies: std::sync::RwLock<Option<String>>,
}

impl DouyuAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            alias_cache: Mutex::new(HashMap::new()),
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
        let numeric = extract_numeric_room_id(&html).ok_or_else(|| {
            anyhow::anyhow!("别名 {} 未解析到数字房间号（房间可能不存在）", room_id)
        })?;

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
            .authed(self.client.get(&url))
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
            cover: crate::adapter::normalize_cover_url(
                find_field_value(json, "room_pic").filter(|s| s.starts_with("http")),
            ),
            started_at: None,
        }
    }

    /// 搜索接口返回的 roomInfo 条目 → 兜底 RoomInfo（状态按索引标记）
    fn room_info_from_rec(rid: &str, ri: &serde_json::Value) -> RoomInfo {
        let started_at = ri["lastShowTime"]
            .as_i64()
            .or_else(|| ri["lastShowTime"].as_str().and_then(|s| s.parse().ok()));
        RoomInfo {
            id: rid.to_string(),
            platform: "douyu".to_string(),
            title: ri["description"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| ri["cateName"].as_str())
                .unwrap_or("Douyu Room")
                .to_string(),
            username: ri["nickName"].as_str().unwrap_or("Unknown").to_string(),
            view_count: 0,
            status: if ri["isLive"].as_i64() == Some(2) {
                LiveStatus::Live
            } else {
                LiveStatus::Sleeping
            },
            cover: crate::adapter::normalize_cover_url(
                ri["roomSrc"]
                    .as_str()
                    .filter(|s| s.starts_with("http"))
                    .map(String::from),
            ),
            started_at,
        }
    }

    /// 通过系统 JS 引擎（node 优先，cscript 兜底）执行当日下发的混淆签名函数。
    /// 混淆参数（XTEA 常量/密钥/后置混淆序列）每日轮换，硬编码公式次日即失效，
    /// 因此必须实时执行服务端返回的 JS。
    async fn sign_via_js(&self, room_id: &str) -> anyhow::Result<String> {
        let swf_url = format!("https://www.douyu.com/swf_api/homeH5Enc?rids={}", room_id);
        let resp = self
            .client
            .get(&swf_url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;
        let sign_func = json
            .get("data")
            .and_then(|d| d.get(&format!("room{}", room_id)))
            .and_then(|v| v.as_str())
            .filter(|s| s.contains("ub98484234"))
            .ok_or_else(|| anyhow::anyhow!("swf_api 响应缺少签名脚本"))?
            .to_string();

        // crypto-js md5 依赖（缓存到临时目录，避免每次下载）
        let crypto_path = std::env::temp_dir().join("streamflow_cryptojs_md5.js");
        if !crypto_path.exists() {
            let body = self
                .client
                .get("https://cdn.jsdelivr.net/npm/cryptojslib@3.1.2/rollups/md5.js")
                .header("User-Agent", USER_AGENT)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            std::fs::write(&crypto_path, &body)?;
        }
        let crypto_js = std::fs::read_to_string(&crypto_path)?;

        let uuid = uuid::Uuid::new_v4().simple().to_string();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();

        // 兼容 node（console.log）与 cscript（WScript.Echo）
        let script = format!(
            "{}\n{}\nvar __r = ub98484234(\"{}\",\"{}\",\"{}\");\nif (typeof WScript !== \"undefined\") {{ WScript.Echo(__r); }} else {{ console.log(__r); }}",
            crypto_js, sign_func, room_id, uuid, ts
        );
        let script_path = std::env::temp_dir().join("streamflow_douyu_sign.js");
        std::fs::write(&script_path, &script)?;

        // node 优先，cscript 兜底
        let output = {
            let node = std::process::Command::new("node")
                .arg(&script_path)
                .output();
            match node {
                Ok(out) if out.status.success() && !out.stdout.is_empty() => out,
                _ => std::process::Command::new("cscript")
                    .arg("//nologo")
                    .arg(&script_path)
                    .output()?,
            }
        };
        if !output.status.success() {
            anyhow::bail!("JS 签名引擎执行失败（需要 node 或系统 cscript）");
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .rev()
            .find(|l| l.contains("sign="))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "签名输出异常: {}",
                    stdout.chars().take(80).collect::<String>()
                )
            })?
            .trim()
            .to_string();
        Ok(line)
    }

    /// 请求指定 rate 的 flv 流地址（签名是房间级的，rate 仅作为表单参数）
    async fn fetch_play_at_rate(
        &self,
        numeric_id: &str,
        sign_query: &str,
        rate: i64,
        referer_room: &str,
    ) -> anyhow::Result<String> {
        let rate_str = rate.to_string();
        let mut form: Vec<(&str, &str)> = vec![("rate", rate_str.as_str())];
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
            .header("Referer", format!("https://www.douyu.com/{}", referer_room))
            .form(&form)
            .send()
            .await?
            .error_for_status()?;
        let json = resp.json::<serde_json::Value>().await?;

        let error = json.get("error").and_then(|v| v.as_i64()).unwrap_or(-1);
        if error != 0 {
            let msg = json
                .get("msg")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            anyhow::bail!("getH5Play(rate={}) 返回错误 {}: {}", rate, error, msg);
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

    /// 多档画质：rate=0（原画，默认/免费最高）+ multirates 其余档位并行请求。
    /// 高档位（蓝光/真4K）若无权限会返回错误，自动跳过该档。
    async fn get_flv_quality_list(
        &self,
        room_id: &str,
    ) -> anyhow::Result<(String, Vec<QualityOption>)> {
        let numeric_id = self.resolve_room_id(room_id).await?;
        let sign_query = self.sign_via_js(&numeric_id).await?;

        // 默认请求 rate=0，同时解析 multirates（各档位 {rate, name}）
        let rate_str = "0";
        let mut form: Vec<(&str, &str)> = vec![("rate", rate_str)];
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
            anyhow::bail!(
                "getH5Play 返回错误 {}: {}",
                error,
                json.get("msg").and_then(|v| v.as_str()).unwrap_or("unknown")
            );
        }
        let data = json
            .get("data")
            .ok_or_else(|| anyhow::anyhow!("响应缺少 data"))?;
        let rtmp_url = data
            .get("rtmp_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("响应缺少 rtmp_url"))?;
        let rtmp_live = data
            .get("rtmp_live")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("响应缺少 rtmp_live"))?;
        let default_url = format!("{}/{}", rtmp_url.trim_end_matches('/'), rtmp_live);

        let multirates: Vec<(i64, String)> = data
            .get("multirates")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        Some((
                            m.get("rate")?.as_i64()?,
                            m.get("name")?.as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 其余档位并行请求（rate=0 已拿到）
        let others: Vec<(i64, String)> = multirates
            .into_iter()
            .filter(|(rate, _)| *rate != 0)
            .take(5)
            .collect();
        let tasks = others
            .iter()
            .map(|(rate, _)| self.fetch_play_at_rate(&numeric_id, &sign_query, *rate, room_id));
        let results = futures_util::future::join_all(tasks).await;
        let mut qualities = vec![QualityOption {
            label: "原画".to_string(),
            url: default_url.clone(),
        }];
        for ((rate, name), result) in others.into_iter().zip(results) {
            match result {
                Ok(url) => {
                    if !qualities.iter().any(|q| q.url == url) {
                        qualities.push(QualityOption { label: name, url });
                    }
                }
                Err(e) => log::warn!("斗鱼档位 {}({}) 不可用：{}", name, rate, e),
            }
        }
        Ok((default_url, qualities))
    }
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
    use crate::adapter::PlatformAdapter;

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
    #[ignore = "依赖真实网络：调试完整流获取链路"]
    async fn debug_stream_9999() {
        let adapter = DouyuAdapter::new();
        match adapter.get_flv_quality_list("9999").await {
            Ok((default_url, qualities)) => {
                eprintln!("FLV OK（{} 档）: {}", qualities.len(), &default_url[..default_url.len().min(140)]);
                for q in &qualities {
                    eprintln!("  - {}", q.label);
                }
            }
            Err(e) => eprintln!("FLV FAIL: {}", e),
        }
    }

    #[tokio::test]
    #[ignore = "依赖真实网络：调试 betard 响应结构"]
    async fn debug_betard_24422() {
        let adapter = DouyuAdapter::new();
        let json = adapter
            .fetch_betard("24422")
            .await
            .expect("betard 请求失败");
        let text = serde_json::to_string(&json).expect("序列化失败");
        println!("LEN: {}", text.len());
        println!("HEAD: {}", &text[..text.len().min(400)]);
        println!("has room_name: {}", text.contains("room_name"));
        println!("has nickname: {}", text.contains("nickname"));
    }

    /// 网络集成测试：关键词搜索应返回含房间条目的结果
    #[tokio::test]
    #[ignore] // 需要真实网络；cargo test -- --ignored 验证
    async fn search_by_keyword_returns_rooms() {
        let adapter = DouyuAdapter::new();
        let results = adapter
            .search("英雄联盟")
            .await
            .expect("斗鱼关键词搜索失败");
        assert!(!results.is_empty(), "斗鱼关键词搜索应返回非空结果");
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
    async fn search_by_room_id_douyu_still_works() {
        let adapter = DouyuAdapter::new();
        let results = adapter.search("9999").await.expect("斗鱼房间搜索失败");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "9999");
    }
}

#[async_trait]
impl crate::adapter::PlatformAdapter for DouyuAdapter {
    fn name(&self) -> &'static str {
        "douyu"
    }

    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        crate::adapter::PlatformCapability::SEARCH
            | crate::adapter::PlatformCapability::ROOM_DETAIL
            | crate::adapter::PlatformCapability::LIVE_STATUS
            | crate::adapter::PlatformCapability::STREAM_URLS
            | crate::adapter::PlatformCapability::EXTERNAL_ONLY
    }

    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        let kw = keyword.trim();
        if kw.is_empty() {
            return Ok(vec![]);
        }

        // 1) 房间号/别名直达（如 9999、pdd），命中即返回唯一结果
        if let Ok(numeric_id) = self.resolve_room_id(kw).await {
            return match self.fetch_betard(&numeric_id).await {
                Ok(json) => {
                    let mut info = self.parse_room_info(&numeric_id, &json);
                    info.id = numeric_id;
                    Ok(vec![info])
                }
                Err(e) => {
                    log::warn!("斗鱼房间 {} 信息获取失败（{}）", numeric_id, e);
                    Ok(vec![])
                }
            };
        }

        // 2) 关键词搜索：apiv2 移动端网关（www 端 getSearchRecV2 仅返回推荐条目，
        //    apiv2 端 recList 字段更全，与斗鱼官方搜索行为一致：无匹配主播时返回模糊结果）
        let url = format!(
            "https://apiv2.douyucdn.cn/japi/search/api/getSearchRecV2?kw={}&type=1&pn=1&rn=30",
            urlencoding::encode(kw)
        );
        let resp = self
            .authed(self.client.get(&url))
            .header("User-Agent", MOBILE_USER_AGENT)
            .header("Referer", "https://m.douyu.com/")
            .send()
            .await?
            .error_for_status()?;
        let json: serde_json::Value = resp.json().await?;
        let rec_list = json["data"]["recList"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if rec_list.is_empty() {
            log::info!("斗鱼关键词 {} 无搜索结果", kw);
            return Ok(vec![]);
        }

        // 3) 搜索索引的 isLive 可能滞后；对前 SEARCH_VERIFY_COUNT 条并发请求
        //    betard 校验真实状态/标题/封面
        const SEARCH_VERIFY_COUNT: usize = 8;
        let verify_ids: Vec<String> = rec_list
            .iter()
            .filter_map(|e| e["roomInfo"]["rid"].as_i64().map(|id| id.to_string()))
            .take(SEARCH_VERIFY_COUNT)
            .collect();
        let details =
            futures::future::join_all(verify_ids.iter().map(|id| self.fetch_betard(id))).await;
        let mut verify_iter = details.into_iter();

        let mut results: Vec<RoomInfo> = Vec::new();
        for e in rec_list.iter() {
            let rid = match e["roomInfo"]["rid"].as_i64() {
                Some(id) => id.to_string(),
                None => continue,
            };
            let info = match verify_iter.next() {
                Some(Ok(json)) => {
                    let mut info = self.parse_room_info(&rid, &json);
                    info.id = rid.clone();
                    info
                }
                Some(Err(err)) => {
                    log::warn!("斗鱼搜索结果 {} 校验失败：{}，使用索引数据", rid, err);
                    Self::room_info_from_rec(&rid, &e["roomInfo"])
                }
                None => Self::room_info_from_rec(&rid, &e["roomInfo"]),
            };
            results.push(info);
        }
        Ok(results)
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let numeric_id = self.resolve_room_id(room_id).await?;
        let json = self.fetch_betard(&numeric_id).await?;
        let mut info = self.parse_room_info(&numeric_id, &json);
        info.id = numeric_id;
        Ok(info)
    }

    async fn stream_urls(&self, room_id: &str) -> AdapterResult {
        match self.get_flv_quality_list(room_id).await {
            Ok((_default_url, qualities)) => Ok(StreamResult {
                platform: "douyu".to_string(),
                room_id: room_id.to_string(),
                // 首项 = 原画（rate=0，默认/免费最高），与 quality[0] 一致
                url: qualities.iter().map(|q| q.url.clone()).collect(),
                quality: qualities,
            }),
            Err(e) => {
                log::warn!("斗鱼流获取失败（{}），走外部浏览器兜底", e);
                Err(AdapterError::ExternalOnly {
                    url: format!("https://www.douyu.com/{}", room_id),
                })
            }
        }
    }

    /// 热门直播间（gapi/rkc/directory/mixList，实测无需登录）
    /// 取首页前 12 条；封面在 rs1 字段（avif/jpg，WebView2 均支持）
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let url = "https://www.douyu.com/gapi/rkc/directory/mixList/0_0/1";
        let resp = self
            .authed(self.client.get(url))
            .header("User-Agent", USER_AGENT)
            .header("Referer", "https://www.douyu.com/")
            .send()
            .await?
            .error_for_status()?;
        let json: serde_json::Value = resp.json().await?;

        let mut rooms = Vec::new();
        if let Some(items) = json.pointer("/data/rl").and_then(|v| v.as_array()) {
            for item in items.iter().take(12) {
                // rid/ol 可能是数字也可能是字符串，宽容解析
                let id = match &item["rid"] {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    _ => continue,
                };
                let view_count = match &item["ol"] {
                    serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
                    serde_json::Value::String(s) => s.parse().unwrap_or(0),
                    _ => 0,
                };
                rooms.push(RoomInfo {
                    id,
                    platform: "douyu".to_string(),
                    title: item["rn"].as_str().unwrap_or("Douyu Live").to_string(),
                    username: item["nn"].as_str().unwrap_or("Unknown").to_string(),
                    view_count,
                    status: LiveStatus::Live,
                    cover: crate::adapter::normalize_cover_url(
                        item["rs1"]
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

    /// 关注列表（wgapi follow/list，需登录 cookie）
    /// 返回已关注主播的直播间（含未开播），最多 5 页
    async fn follows(&self) -> anyhow::Result<Vec<RoomInfo>> {
        let cookie = self
            .cookies
            .read()
            .unwrap()
            .clone()
            .filter(|c| !c.is_empty())
            .ok_or_else(|| anyhow::anyhow!("未登录斗鱼，请先在设置中心扫码登录"))?;

        let mut rooms: Vec<RoomInfo> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for page in 1..=5 {
            let url = format!(
                "https://www.douyu.com/wgapi/livenc/liveweb/follow/list?page={}&sort=0&cid1=0",
                page
            );
            let resp = self
                .client
                .get(&url)
                .header("User-Agent", USER_AGENT)
                .header("Referer", "https://www.douyu.com/directory/myFollow")
                .header("Cookie", &cookie)
                .send()
                .await?
                .error_for_status()?;
            let json: serde_json::Value = resp.json().await?;
            if json.get("error").and_then(|v| v.as_i64()) != Some(0) {
                anyhow::bail!("斗鱼关注列表返回异常: {}", json["msg"]);
            }
            let list = json
                .pointer("/data/list")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if list.is_empty() {
                break;
            }
            let count = list.len();
            let limit = json
                .pointer("/data/limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(count as i64);
            for item in &list {
                let id = match &item["room_id"] {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    _ => continue,
                };
                if !seen.insert(id.clone()) {
                    continue;
                }
                let username = item["nickname"].as_str().unwrap_or("Unknown").to_string();
                let title = item["room_name"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .unwrap_or_else(|| format!("{} 的直播间", username));
                let view_count = match &item["online"] {
                    serde_json::Value::Number(n) => n.as_u64().unwrap_or(0),
                    serde_json::Value::String(s) => s.parse().unwrap_or(0),
                    _ => 0,
                };
                // show_status: 1=直播中 2=未开播
                let live = item["show_status"].as_i64() == Some(1);
                let cover_url = item["rs16_avif"]
                    .as_str()
                    .filter(|s| s.starts_with("http"))
                    .or_else(|| item["room_src"].as_str().filter(|s| s.starts_with("http")));
                rooms.push(RoomInfo {
                    id,
                    platform: "douyu".to_string(),
                    title,
                    username,
                    view_count,
                    status: if live {
                        LiveStatus::Live
                    } else {
                        LiveStatus::Sleeping
                    },
                    cover: crate::adapter::normalize_cover_url(cover_url.map(String::from)),
                    started_at: item["show_time"].as_i64(),
                });
            }
            if (count as i64) < limit || limit <= 0 {
                break;
            }
        }
        Ok(rooms)
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

    fn set_cookies(&self, cookie: Option<String>) {
        *self.cookies.write().unwrap() = cookie;
    }
}
