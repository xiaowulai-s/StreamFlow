//! Douyu 斗鱼适配器
//! 房间信息使用 betard 接口（实测可用）；
//! 流地址通过 swf_api 取签名密钥，按 reverse::douyu_signature 计算 sign，
//! POST lapi/live/getH5Play 获取 flv 直播流（实测 error=0）。

use crate::reverse::douyu_signature;
use crate::{AdapterError, AdapterResult, LiveStatus, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Clone)]
pub struct DouyuAdapter {
    client: Client,
}

impl DouyuAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
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
        let get_str = |key: &str| json.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());

        let title = get_str("room_name").unwrap_or_else(|| "Douyu Room".to_string());
        let username = get_str("nickname").unwrap_or_else(|| "Unknown Streamer".to_string());
        // showStatus 为字符串 "1"（直播中）
        let is_live = get_str("showStatus").as_deref() == Some("1");

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

    /// 获取 flv 直播流地址
    async fn get_flv_stream_url(&self, room_id: &str) -> anyhow::Result<String> {
        let key = self.fetch_sign_key(room_id).await?;
        let uuid = uuid::Uuid::new_v4().simple().to_string();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();
        let sign_query = douyu_signature::sign(room_id, &uuid, &ts, &key);

        // sign_query 形如 "v=...&did=...&tt=...&sign=..."，拆成表单键值
        let mut form: Vec<(&str, &str)> = vec![("rate", "0")];
        for pair in sign_query.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                form.push((k, v));
            }
        }

        let url = format!("https://www.douyu.com/lapi/live/getH5Play/{}", room_id);
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

impl Default for DouyuAdapter {
    fn default() -> Self {
        Self::new()
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
        // 搜索接口均已下线（c.douyu.com 域名不解析，japi 变体均 404）
        log::info!("斗鱼搜索接口已下线，关键词 {} 返回空结果，可使用房间号直达", keyword);
        Ok(vec![])
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let json = self.fetch_betard(room_id).await?;
        Ok(self.parse_room_info(room_id, &json))
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
        let json = self.fetch_betard(room_id).await?;
        let info = self.parse_room_info(room_id, &json);
        Ok(info.status)
    }

    /// 斗鱼弹幕协议未接入，返回 None（前端按无弹幕处理）
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }
}
