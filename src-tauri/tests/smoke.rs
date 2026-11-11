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
    let info = adapter
        .room_detail("6")
        .await
        .expect("获取 B 站房间信息失败");
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
async fn bilibili_keyword_search_smoke() {
    // 关键词搜索（主页取 cookie → search/type）：搜"悲喜"应返回主播条目
    let adapter = BilibiliAdapter::new();
    let rooms = adapter.search("悲喜").await.expect("B 站关键词搜索失败");
    assert!(!rooms.is_empty(), "关键词 悲喜 应命中至少一个主播");
    assert!(
        rooms.iter().any(|r| r.username.contains("悲喜")),
        "结果应包含包含关键词的主播名"
    );
}

/// 读取本机应用登录态中的平台 cookie（未登录返回 None）
fn saved_cookie(platform: &str) -> Option<String> {
    let base = std::env::var("LOCALAPPDATA").ok()?;
    let path = std::path::PathBuf::from(base)
        .join("com.streamflow.app")
        .join("auth.json");
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    json.pointer(&format!("/platforms/{}/cookie", platform))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from)
}

#[tokio::test]
#[ignore = "依赖真实网络与本机登录态"]
async fn douyu_follows_smoke() {
    let Some(cookie) = saved_cookie("douyu") else {
        return;
    };
    let adapter = DouyuAdapter::new();
    adapter.set_cookies(Some(cookie));
    let rooms = adapter.follows().await.expect("斗鱼关注列表获取失败");
    assert!(!rooms.is_empty(), "斗鱼关注列表不应为空");
    assert!(rooms.iter().all(|r| r.platform == "douyu"));
}

#[tokio::test]
#[ignore = "依赖真实网络与本机登录态"]
async fn bilibili_follows_smoke() {
    let Some(cookie) = saved_cookie("bilibili") else {
        return;
    };
    let adapter = BilibiliAdapter::new();
    adapter.set_cookies(Some(cookie));
    let rooms = adapter.follows().await.expect("B站关注列表获取失败");
    assert!(rooms.iter().all(|r| r.platform == "bilibili"));
}

#[tokio::test]
#[ignore = "依赖真实网络与本机登录态"]
async fn huya_follows_smoke() {
    let Some(cookie) = saved_cookie("huya") else {
        return;
    };
    let adapter = HuyaAdapter::new();
    adapter.set_cookies(Some(cookie));
    let rooms = adapter.follows().await.expect("虎牙订阅列表获取失败");
    assert!(!rooms.is_empty(), "虎牙订阅列表不应为空");
    assert!(rooms.iter().all(|r| r.platform == "huya"));
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
    let result = adapter
        .stream_urls("lpl")
        .await
        .expect("获取虎牙流地址失败");

    assert!(!result.url.is_empty(), "流地址列表为空");
    let url = &result.url[0];
    assert!(url.contains(".m3u8"), "应为 m3u8 地址: {}", url);

    // 实际拉流验证签名有效性（HTTP 200 且返回播放列表）
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .header("Referer", "https://www.huya.com/")
        .send()
        .await
        .expect("请求 m3u8 失败")
        .error_for_status()
        .expect("m3u8 拉流被拒绝（签名无效）");
    let body = resp.text().await.expect("读取 m3u8 内容失败");
    assert!(
        body.starts_with("#EXTM3U"),
        "响应不是合法播放列表: {}",
        &body[..body.len().min(80)]
    );
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn huya_room_detail_smoke() {
    let adapter = HuyaAdapter::new();
    let info = adapter
        .room_detail("lpl")
        .await
        .expect("获取虎牙房间信息失败");
    assert_eq!(info.platform, "huya");
    assert!(!info.title.is_empty());
}

#[tokio::test]
#[ignore = "依赖真实网络"]
async fn douyu_room_detail_smoke() {
    let adapter = DouyuAdapter::new();
    let info = adapter
        .room_detail("9999")
        .await
        .expect("获取斗鱼房间信息失败");
    assert_eq!(info.platform, "douyu");
    assert!(!info.title.is_empty());
}

/// 虎牙/斗鱼热门列表：应返回非空且带封面
#[tokio::test]
#[ignore = "依赖真实网络"]
async fn hot_rooms_smoke() {
    let huya = HuyaAdapter::new();
    let douyu = DouyuAdapter::new();

    let huya_hot = huya.hot_rooms().await.expect("虎牙热门失败");
    assert!(!huya_hot.is_empty(), "虎牙热门不应为空");
    assert!(
        huya_hot.iter().any(|r| r.cover.is_some()),
        "虎牙热门应含封面"
    );
    println!(
        "虎牙 hot: {} 条，首条 {:?}",
        huya_hot.len(),
        huya_hot[0].username
    );

    let douyu_hot = douyu.hot_rooms().await.expect("斗鱼热门失败");
    assert!(!douyu_hot.is_empty(), "斗鱼热门不应为空");
    assert!(
        douyu_hot.iter().any(|r| r.cover.is_some()),
        "斗鱼热门应含封面"
    );
    println!(
        "斗鱼 hot: {} 条，首条 {:?}",
        douyu_hot.len(),
        douyu_hot[0].username
    );
}

/// 封面链路验证：各平台 search / hot_rooms 返回的房间应带真实封面 URL
#[tokio::test]
#[ignore = "依赖真实网络"]
async fn cover_fields_smoke() {
    let huya = HuyaAdapter::new();
    let douyu = DouyuAdapter::new();
    let bili = BilibiliAdapter::new();

    let huya_rooms = huya.search("988").await.expect("虎牙搜索失败");
    println!(
        "虎牙 search cover: {:?}",
        huya_rooms.first().map(|r| r.cover.clone())
    );

    let douyu_rooms = douyu.search("英雄联盟").await.expect("斗鱼搜索失败");
    let douyu_covers = douyu_rooms.iter().filter(|r| r.cover.is_some()).count();
    println!("斗鱼 search: {}/{} 有封面", douyu_covers, douyu_rooms.len());

    let bili_rooms = bili.search("英雄联盟").await.expect("B站搜索失败");
    let bili_covers = bili_rooms.iter().filter(|r| r.cover.is_some()).count();
    println!("B站 search: {}/{} 有封面", bili_covers, bili_rooms.len());

    let bili_hot = bili.hot_rooms().await.expect("B站热门失败");
    let hot_covers = bili_hot.iter().filter(|r| r.cover.is_some()).count();
    println!("B站 hot: {}/{} 有封面", hot_covers, bili_hot.len());
    assert!(
        hot_covers > 0 && douyu_covers > 0 && bili_covers > 0,
        "至少一个平台封面链路断裂"
    );
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
