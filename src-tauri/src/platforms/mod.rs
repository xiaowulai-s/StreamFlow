//! 平台适配器模块
//! 所有直播平台的适配器实现

pub mod bilibili;
pub mod douyin;
pub mod douyu;
pub mod huya;
pub mod kuaishou;
pub mod mock;

pub use bilibili::BilibiliAdapter;
pub use douyin::DouyinAdapter;
pub use douyu::DouyuAdapter;
pub use huya::HuyaAdapter;
pub use kuaishou::KuaishouAdapter;
pub use mock::MockAdapter;
