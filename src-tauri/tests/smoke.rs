//! 平台适配器端到端冒烟测试
//! 依赖真实网络，默认 #[ignore]，运行：cargo test --test smoke -- --ignored

use streamflow_lib::adapter::PlatformAdapter;
use streamflow_lib::platforms::bilibili::BilibiliAdapter;
use streamflow_lib::platforms::douyu::DouyuAdapter;
use streamflow_lib::platforms::huya::HuyaAdapter;

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn bilibili_stream_urls_smoke() {
    let adapter = BilibiliAdapter::new();
    let result = adapter
        .stream_urls("6")
        .await
        .expect("获取 B 站流地址失败（房间未开播或 API 变更）");
    assert!(!result.url.is_empty(), "流地址列表为空");
    assert!(
        result.url[0].contains(".m3u8") || result.url[0].contains(".flv"),
        "应为 m3u8/flv 地址: {}",
        result.url[0]
    );
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn bilibili_room_detail_smoke() {
    let adapter = BilibiliAdapter::new();
    let info = adapter.room_detail("6").await.expect("获取 B 站房间信息失败");
    assert_eq!(info.platform, "bilibili");
    assert!(!info.title.is_empty());
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn douyu_alias_resolution_smoke() {
    // 别名 pigff → 数字房间号 24422（yt-dlp 已知数据），返回真实房间卡片
    let adapter = DouyuAdapter::new();
    let rooms = adapter.search("pigff").await.expect("别名解析失败");
    assert!(!rooms.is_empty(), "别名 pigff 应解析出真实房间卡片");
    assert_eq!(rooms[0].id, "24422");
    assert_ne!(rooms[0].title, "Douyu Room", "应返回真实房间标题");
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn bilibili_keyword_rejected() {
    // B 站仅支持数字房间号：非数字关键词应返回明确错误
    let adapter = BilibiliAdapter::new();
    let err = adapter
        .search("lpl")
        .await
        .expect_err("B 站非数字关键词应返回错误");
    assert!(
        err.to_string().contains("数字房间号"),
        "错误信息应说明仅支持数字房间号: {}",
        err
    );
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn huya_alias_smoke() {
    // 虎牙别名 lpl 直达（频道页），应返回真实房间卡片
    let adapter = HuyaAdapter::new();
    let rooms = adapter.search("lpl").await.expect("虎牙别名解析失败");
    assert!(!rooms.is_empty(), "别名 lpl 应解析出真实房间卡片");
    assert_ne!(rooms[0].title, "Unknown Title", "应返回真实房间标题");
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn huya_stream_urls_smoke() {
    let adapter = HuyaAdapter::new();
    let result = adapter.stream_urls("lpl").await.expect("获取虎牙流地址失败");

    assert!(!result.url.is_empty(), "流地址列表为空");
    let url = &result.url[0];
    assert!(url.contains(".m3u8"), "应为 m3u8 地址: {}", url);

    // 实际拉流验证签名有效性（HTTP 200 且返回播放列表）
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .header("Referer", "https://www.huya.com/")
        .send()
        .await
        .expect("请求 m3u8 失败")
        .error_for_status()
        .expect("m3u8 拉流被拒绝（签名无效）");
    let body = resp.text().await.expect("读取 m3u8 内容失败");
    assert!(body.starts_with("#EXTM3U"), "响应不是合法播放列表: {}", &body[..body.len().min(80)]);
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn huya_room_detail_smoke() {
    let adapter = HuyaAdapter::new();
    let info = adapter.room_detail("lpl").await.expect("获取虎牙房间信息失败");
    assert_eq!(info.platform, "huya");
    assert!(!info.title.is_empty());
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn douyu_room_detail_smoke() {
    let adapter = DouyuAdapter::new();
    let info = adapter.room_detail("9999").await.expect("获取斗鱼房间信息失败");
    assert_eq!(info.platform, "douyu");
    assert!(!info.title.is_empty());
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn douyu_stream_urls_smoke() {
    let adapter = DouyuAdapter::new();
    let result = adapter
        .stream_urls("9999")
        .await
        .expect("获取斗鱼流地址失败（房间未开播或签名失效）");
    assert!(!result.url.is_empty(), "流地址列表为空");
    assert!(
        result.url[0].contains(".flv"),
        "应为 flv 地址: {}",
        result.url[0]
    );
}

