//! Mock 平台适配器
//! 用于离线开发和测试

use crate::{LiveStatus, QualityOption, RoomInfo, StreamResult};
use async_trait::async_trait;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Clone)]
pub struct MockAdapter {
    pub name: String,
}

impl MockAdapter {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

impl Default for MockAdapter {
    fn default() -> Self {
        Self::new("mock")
    }
}

#[async_trait]
impl crate::adapter::PlatformAdapter for MockAdapter {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        crate::adapter::PlatformCapability::all()
    }

    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        sleep(Duration::from_millis(100)).await;

        Ok(vec![
            RoomInfo {
                id: format!("mock_{}", keyword),
                platform: self.name.clone(),
                title: format!("Demo Room: {}", keyword),
                username: "Mock Master".to_string(),
                view_count: 1000,
                status: LiveStatus::Live,
                cover: None,
                started_at: None,
            },
            RoomInfo {
                id: "mock_2".to_string(),
                platform: self.name.clone(),
                title: "Another Mock Room".to_string(),
                username: "Another Mock".to_string(),
                view_count: 500,
                status: LiveStatus::Sleeping,
                cover: None,
                started_at: None,
            },
        ])
    }

    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        sleep(Duration::from_millis(50)).await;

        Ok(RoomInfo {
            id: room_id.to_string(),
            platform: self.name.clone(),
            title: format!("Mock Room {}", room_id),
            username: "Mock Master".to_string(),
            view_count: 1234,
            status: LiveStatus::Live,
            cover: None,
            started_at: None,
        })
    }

    async fn stream_urls(&self, room_id: &str) -> crate::AdapterResult {
        sleep(Duration::from_millis(100)).await;

        Ok(StreamResult {
            platform: self.name.clone(),
            room_id: room_id.to_string(),
            url: vec![
                "https://example.com/mock/playlist.m3u8".to_string(),
                "https://example.com/mock/stream.flv".to_string(),
            ],
            quality: vec![
                QualityOption {
                    label: "1080P".to_string(),
                    url: "https://example.com/mock/1080p.m3u8".to_string(),
                },
                QualityOption {
                    label: "720P".to_string(),
                    url: "https://example.com/mock/720p.m3u8".to_string(),
                },
            ],
        })
    }

    async fn live_status(&self, _room_id: &str) -> anyhow::Result<LiveStatus> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now % 2 == 0 {
            Ok(LiveStatus::Live)
        } else {
            Ok(LiveStatus::Sleeping)
        }
    }

    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        Some("wss://mock.danmaku.url/ws".to_string())
    }
}
