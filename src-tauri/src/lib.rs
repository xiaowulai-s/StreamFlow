//! StreamFlow Rust Backend
//! Multi-platform live streaming aggregator

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

pub mod adapter;
pub mod platforms;
pub mod queue;
pub mod database;
pub mod reverse;
pub mod commands;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomInfo {
    pub id: String,
    pub platform: String,
    pub title: String,
    pub username: String,
    pub view_count: u64,
    pub status: LiveStatus,
    /// 房间封面（接口提供时才有）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LiveStatus {
    Live,
    Sleeping,
    Error(String),
}

impl Default for LiveStatus {
    fn default() -> Self {
        LiveStatus::Sleeping
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamResult {
    pub platform: String,
    pub room_id: String,
    pub url: Vec<String>,
    pub quality: Vec<QualityOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityOption {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterError {
    RateLimited,
    #[serde(rename = "network_error")]
    NetworkError(String),
    #[serde(rename = "parse_error")]
    ParseError(String),
    #[serde(rename = "external_only")]
    ExternalOnly { url: String },
}

impl From<anyhow::Error> for AdapterError {
    fn from(e: anyhow::Error) -> Self {
        AdapterError::NetworkError(e.to_string())
    }
}

impl From<reqwest::Error> for AdapterError {
    fn from(e: reqwest::Error) -> Self {
        AdapterError::NetworkError(e.to_string())
    }
}

pub type AdapterResult = Result<StreamResult, AdapterError>;

/// 应用配置
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub ad_filter_enabled: bool,
    pub danmu_visible: bool,
    pub default_quality: String,
}

/// 应用状态
pub struct AppState {
    pub rooms: Arc<RwLock<Vec<RoomInfo>>>,
    pub favorites: Arc<RwLock<Vec<String>>>,
    pub settings: Arc<RwLock<Settings>>,
    pub adapters: adapter::AdapterRegistry,
}

pub fn init() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    Ok(())
}
