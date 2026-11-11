//! Platform Adapter Trait and Registry
//! 平台适配器 Trait 和注册表

use crate::{AdapterResult, LiveStatus, RoomInfo};
use async_trait::async_trait;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PlatformCapability: u32 {
        const SEARCH = 1 << 0;
        const ROOM_DETAIL = 1 << 1;
        const STREAM_URLS = 1 << 2;
        const LIVE_STATUS = 1 << 3;
        const DANMAKU = 1 << 4;
        const USER_PROFILE = 1 << 5;
        const EXTERNAL_ONLY = 1 << 6;
    }
}

#[async_trait]
pub trait PlatformAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> PlatformCapability;
    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>>;
    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo>;
    async fn stream_urls(&self, room_id: &str) -> AdapterResult;
    async fn live_status(&self, room_id: &str) -> anyhow::Result<LiveStatus>;
    async fn danmaku_url(&self, room_id: &str) -> Option<String>;

    /// 热门/推荐直播间列表（平台支持时实现，默认不支持返回空）
    async fn hot_rooms(&self) -> anyhow::Result<Vec<RoomInfo>> {
        Ok(vec![])
    }
}

pub struct AdapterRegistry {
    adapters: Vec<Box<dyn PlatformAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }
    
    pub fn register<A: PlatformAdapter + 'static>(&mut self, adapter: A) {
        self.adapters.push(Box::new(adapter));
    }
    
    pub fn get(&self, name: &str) -> Option<&Box<dyn PlatformAdapter>> {
        self.adapters.iter().find(|a| a.name() == name)
    }
    
    pub fn all(&self) -> &[Box<dyn PlatformAdapter>] {
        &self.adapters
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
