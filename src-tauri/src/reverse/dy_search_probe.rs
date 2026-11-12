// 临时探针：迭代抖音搜索参数组合
use crate::reverse::douyin_abogus::{get_value, USER_AGENT};
use serde_json::Value;

fn read_cookie() -> String {
    let dir = std::env::var("LOCALAPPDATA").unwrap();
    let path = std::path::Path::new(&dir).join("com.streamflow.app").join("auth.json");
    let store: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    store["platforms"]["douyin"]["cookie"].as_str().unwrap_or("").to_string()
}

#[tokio::test]
#[ignore]
async fn probe_variants() {
    let cookie_login = read_cookie();
    if cookie_login.is_empty() { eprintln!("无登录态"); return; }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(12)).build().unwrap();
    let ua = USER_AGENT;
    let home = client.get("https://www.douyin.com/").header("User-Agent", ua).send().await.unwrap();
    let ttwid: Vec<String> = home.headers().get_all(reqwest::header::SET_COOKIE).iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .filter(|s| s.starts_with("ttwid=") || s.starts_with("msToken=")).collect();
    let cookie_header = format!("{}{}{}", ttwid.join("; "), if ttwid.is_empty() { "" } else { "; " }, cookie_login);

    let base = "device_platform=webapp&aid=6383&channel=channel_pc_web&cookie_enabled=true&screen_width=1920&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=130.0.0.0&browser_online=true&engine_name=Blink&engine_version=130.0.0.0&os_name=Windows&os_version=10&cpu_core_num=12&memory_info=8&platform=PC&downlink=10&effective_type=4g&round_trip_time=50";
    let variants: Vec<(&str, String)> = vec![
        ("live/aweme_live 王者荣耀", format!("{}&search_channel=aweme_live&keyword={}&search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=10&pc_client_type=1&version_code=170400&version_name=17.4.0", base, urlencoding::encode("王者荣耀"))),
        ("live/aweme_live_general 王者荣耀", format!("{}&search_channel=aweme_live_general&keyword={}&search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=10&pc_client_type=1&version_code=170400&version_name=17.4.0", base, urlencoding::encode("王者荣耀"))),
        ("general/single live类型 王者荣耀", format!("{}&search_channel=aweme_general&search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=10&pc_client_type=1&version_code=170400&version_name=17.4.0&sort_type=0&publish_time=0&keyword={}", base, urlencoding::encode("王者荣耀"))),
    ];

    for (name, params) in variants {
        let path = if name.starts_with("general") { "/aweme/v1/web/general/search/single/" } else { "/aweme/v1/web/live/search/" };
        let a_bogus = get_value(&params, "GET");
        let url = format!("https://www.douyin.com{}?{}&a_bogus={}", path, params, urlencoding::encode(&a_bogus));
        let resp = client.get(&url)
            .header("User-Agent", ua)
            .header("Referer", "https://www.douyin.com/")
            .header("Cookie", cookie_header.clone())
            .send().await.unwrap();
        let body = resp.text().await.unwrap_or_default();
        // 提取关键信息
        let (code, datainfo) = match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(j) => (
                j.pointer("/status_code").and_then(|v| v.as_i64()).map(|v| v.to_string()).unwrap_or("?".into()),
                // 找出所有可能装结果的路径
                ["data", "/data/data", "/data/aweme_list", "/data/live_list", "/data/data"]
                    .iter()
                    .map(|p| {
                        let v = if *p == "data" { j.get("data") } else { j.pointer(p) };
                        format!("{}={}", p, match v {
                            Some(Value::Array(a)) => format!("arr[{}]", a.len()),
                            Some(Value::Object(o)) => format!("obj{{{}}}", o.keys().cloned().collect::<Vec<_>>().join(",").chars().take(80).collect::<String>()),
                            Some(Value::Null) | None => "null".into(),
                            Some(_) => "other".into(),
                        })
                    })
                    .collect::<Vec<String>>().join(" | "),
            ),
            Err(_) => ("非JSON".into(), body.chars().take(80).collect()),
        };
        eprintln!("【{}】code={} {}", name, code, datainfo);
    }
}

#[tokio::test]
#[ignore]
async fn probe_following_live() {
    let cookie_login = read_cookie();
    if cookie_login.is_empty() { eprintln!("无登录态"); return; }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(12)).build().unwrap();
    let ua = USER_AGENT;
    // 关注的直播中列表（live 域接口，试两种路径）
    let urls = [
        "https://live.douyin.com/webcast/user/following/live/?aid=6383&count=15&offset=0",
        "https://www.douyin.com/aweme/v1/web/follow/feed/?aid=6383&count=10&cursor=0",
    ];
    for url in urls {
        let resp = client.get(url)
            .header("User-Agent", ua)
            .header("Referer", "https://live.douyin.com/")
            .header("Cookie", cookie_login.clone())
            .send().await.unwrap();
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        eprintln!("【{}】status={} body={}", url.chars().skip(30).take(45).collect::<String>(), status, &body.chars().take(220).collect::<String>().replace('\n', ""));
    }
}
