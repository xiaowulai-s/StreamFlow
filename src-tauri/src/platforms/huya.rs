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

#[derive(Clone)]
pub struct HuyaAdapter {
    client: Client,
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

impl HuyaAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }

    async fn fetch_room_page(&self, room_id: &str) -> anyhow::Result<String> {
        let url = format!("https://www.huya.com/{}", room_id);
        let resp = self
            .client
            .get(&url)
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
        // 官方搜索接口已下线（返回 "not supported function"）。
        // 虎牙房间页 URL 天然支持别名/短号直达（如 huya.com/lpl），按房间直达解析：
        // 解析成功返回真实房间卡片，失败返回空列表。
        match self.room_detail(keyword).await {
            Ok(info) => Ok(vec![info]),
            Err(e) => {
                log::info!("虎牙关键词 {} 无法解析为房间：{}", keyword, e);
                Ok(vec![])
            }
        }
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        let text = self.fetch_room_page(room_id).await?;
        let live_idx = text.find("\"gameLiveInfo\":").ok_or_else(|| anyhow::anyhow!("页面缺少 gameLiveInfo"))?;

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
            cover: find_json_str(&text, "screenshot", live_idx).filter(|s| s.starts_with("http")),
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

    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus> {
        let text = self.fetch_room_page(room_id).await?;
        let is_live = Self::extract_stream_entry(&text).is_some();

        Ok(if is_live {
            LiveStatus::Live
        } else {
            LiveStatus::Sleeping
        })
    }

    /// 虎牙弹幕为 TC 二进制协议，暂未接入，返回 None（前端按无弹幕处理）
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        None
    }
}
