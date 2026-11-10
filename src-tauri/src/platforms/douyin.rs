//! Douyin 平台适配器
//! X-Bogus 指纹 + 私有协议
//! 风险：抖音反爬极强，使用 headless 浏览器或指纹生成

use crate::{AdapterError, AdapterResult, LiveStatus, RoomInfo, StreamResult};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

#[derive(Clone)]
pub struct DouyinAdapter {
    client: Client,
}

impl DouyinAdapter {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(15))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
                .build()
                .unwrap(),
        }
    }
    
    // X-Bogus 指纹生成器
    // 实际项目中需要集成 x-bogus 算法或使用 headless 浏览器获取
    fn generate_x_bogus(&self, url: &str) -> String {
        // 占位符 - 实际使用时需要集成:
        // 1. x-bogus 逆向算法
        // 2. 或者使用 puppeteer/scroll 实现
        // 3. 或者调用外部服务
        "placeholder_x_bogus".to_string()
    }
    
    async fn fetch_room_info(&self, room_id: &str) -> anyhow::Result<serde_json::Value> {
        let url = format!(
            "https://web.douyin.com/wb/2340819771833756875/?room_id={}",
            room_id
        );
        
        let resp = self.client
            .get(&url)
            .header("X-Bogus", self.generate_x_bogus(&url))
            .send()
            .await?;
            
        let json = resp.json::<serde_json::Value>().await?;
        Ok(json)
    }
    
    // 获取直播间信息 - 需要解析页面 JSON
    async fn parse_room_data(&self, html: &str) -> Option<(String, String, u64)> {
        // 从 HTML 中提取窗口共享数据
        // 实际项目中需要使用正则或 HTML 解析器
        None
    }
}

impl Default for DouyinAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::adapter::PlatformAdapter for DouyinAdapter {
    fn name(&self) -> &'static str {
        "douyin"
    }
    
    fn capabilities(&self) -> crate::adapter::PlatformCapability {
        // 抖音只能外部跳转或逆向获取
        // 当前阶段标记为 ExternalOnly
        crate::adapter::PlatformCapability::SEARCH
            | crate::adapter::PlatformCapability::ROOM_DETAIL
            | crate::adapter::PlatformCapability::EXTERNAL_ONLY
    }
    
    async fn search(&self, keyword: &str) -> anyhow::Result<Vec<RoomInfo>> {
        // 抖音搜索需要特殊处理
        // 暂时返回空结果
        Ok(vec![])
    }
    
    async fn room_detail(&self, room_id: &str) -> anyhow::Result<RoomInfo> {
        // TODO: 实现抖音房间详情获取
        // 需要解析页面中的 JSON 数据
        Err(anyhow::anyhow!("Douyin room detail parsing not implemented"))
    }
    
    async fn stream_urls(&self, _room_id: &str) -> AdapterResult {
        // 抖音流地址获取 - 当前标记为 ExternalOnly
        Err(AdapterError::ExternalOnly {
            url: format!("https://web.douyin.com/room/{}/", _room_id),
        })
    }
    
    async fn live_status(&self, _room_id: &str) -> anyhow::Result<LiveStatus> {
        // 抖音直播状态 - 需要页面解析
        Ok(LiveStatus::Sleeping)
    }
    
    async fn danmaku_url(&self, _room_id: &str) -> Option<String> {
        // 抖音弹幕协议特殊，需反向分析
        None
    }
}
