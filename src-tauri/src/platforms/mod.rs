//! 平台适配器模块
//! 所有直播平台的适配器实现

pub mod bilibili;
pub mod douyu;
pub mod huya;
pub mod douyin;
pub mod kuaishou;
pub mod mock;

pub use bilibili::BilibiliAdapter;
pub use douyu::DouyuAdapter;
pub use huya::HuyaAdapter;
pub use douyin::DouyinAdapter;
pub use kuaishou::KuaishouAdapter;
pub use mock::MockAdapter;
