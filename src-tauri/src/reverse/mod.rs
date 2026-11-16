//! 逆向工具库
//! 用于处理平台加密协议、签名算法等
//!
//! 历史清理（2026-11-16 P2）：douyin_bogus（placeholder）、kuaishou_ws（空壳）、
//! dy_search_probe（临时探针）已移除——抖音搜索/关注/热门现统一走 webctx 页面环境路线，
//! a_bogus 独立签名仅保留 douyin_abogus（douyin.rs 登录态搜索一条路径）。

pub mod douyin_abogus;
pub mod huya_signature;
pub mod webctx;

pub use huya_signature::*;
