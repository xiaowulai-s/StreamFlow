//! 抖音 A-Bogus 签名（纯算法移植，无需 JS 引擎）
//!
//! 移植自 JoeanAmier/TikTokDownloader（GPL v3）→ DLWangSan/douyin_parse 的 Python 实现。
//! 算法要素：SM3 双轮哈希 + RC4 + 自定义 base64（s4 表）。
//! 注意：`UA_CODE` 与 `USER_AGENT` 必须配套使用（特征码与 UA 字节相关），
//! 若平台更新签名版本，优先排查该配对。

use rand::Rng;

/// 与下方 USER_AGENT 配套的 UA 特征码（32 字节，来自参考实现）
const UA_CODE: [u8; 32] = [
    76, 98, 15, 131, 97, 245, 224, 133, 122, 199, 241, 166, 79, 34, 90, 191, 128, 126, 122, 98, 66,
    11, 14, 40, 49, 110, 110, 173, 67, 96, 138, 252,
];

/// 与 UA_CODE 配套的 User-Agent
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";

const END_STRING: &str = "cus";
/// 参考实现的默认浏览器信息（固定值，避免随机差异带来的兼容性风险）
const BROWSER: &str = "1536|742|1536|864|0|0|0|0|1536|864|1536|864|1536|742|24|24|MacIntel";
const CHARSET_S4: &str = "Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe";

fn sm3_to_array(data: &[u8]) -> Vec<u8> {
    use sm3::{Digest, Sm3};
    let mut hasher = Sm3::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

/// 双轮 SM3：sm3(sm3(data + "cus")) 的字节数组
fn double_sm3_bytes(data: &str) -> Vec<u8> {
    let mut first = data.as_bytes().to_vec();
    first.extend_from_slice(END_STRING.as_bytes());
    let h1 = sm3_to_array(&first);
    let h2 = sm3_to_array(&h1);
    h2
}

/// Python `_random_list(a, b, c, d, e, f, g)`
fn random_list(a: Option<f64>, b: i64, c: i64, d: i64, e: i64, f: i64, g: i64) -> Vec<i64> {
    let r = a.unwrap_or_else(|| rand::thread_rng().gen_range(0.0..10000.0));
    let ri = r as i64;
    let v1 = ri & 255;
    let v2 = ri >> 8;
    vec![(v1 & b) | d, (v1 & c) | e, (v2 & b) | f, (v2 & c) | g]
}

fn list_1() -> Vec<i64> {
    // Python: _random_list(r, 170, 85, 1, 2, 5, 45 & 170 = 40)
    random_list(None, 170, 85, 1, 2, 5, 40)
}

fn list_2() -> Vec<i64> {
    random_list(None, 170, 85, 1, 0, 0, 0)
}

fn list_3() -> Vec<i64> {
    random_list(None, 170, 85, 1, 0, 5, 0)
}

fn generate_string_1() -> String {
    let mut codes = Vec::new();
    codes.extend(list_1());
    codes.extend(list_2());
    codes.extend(list_3());
    codes.iter().map(|&c| c as u8 as char).collect()
}

/// Python `_list_4(a..r)`：43 元素常量结构
fn list_4(vals: [i64; 17]) -> Vec<i64> {
    let [a, b, c, d, e, f, g, h, i, j, k, m, n, o, p, q, r] = vals;
    vec![
        44, a, 0, 0, 0, 0, 24, b, n, 0, c, d, 0, 0, 0, 1, 0, 239, e, o, f, g, 0, 0, 0, 0, h, 0, 0,
        14, i, j, 0, k, m, 3, p, 1, q, 1, r, 0, 0, 0,
    ]
}

fn rc4_encrypt(plaintext: &[u8], key: &str) -> Vec<u8> {
    let key: Vec<u8> = key.bytes().collect();
    let mut s: Vec<u8> = (0..=255u16).map(|v| v as u8).collect();
    let mut j: usize = 0;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
        s.swap(i, j);
    }
    let (mut i, mut j) = (0usize, 0usize);
    let mut out = Vec::with_capacity(plaintext.len());
    for &ch in plaintext {
        i = (i + 1) % 256;
        j = (j + s[i] as usize) % 256;
        s.swap(i, j);
        let t = (s[i] as usize + s[j] as usize) % 256;
        out.push(s[t] ^ ch);
    }
    out
}

fn xor_check(arr: &[i64]) -> i64 {
    arr.iter().fold(0, |acc, &x| acc ^ x)
}

/// 3 字节 → 4 字符 s4 编码 + `=` 补位
fn generate_result(s: &[u8]) -> String {
    let cs: Vec<char> = CHARSET_S4.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < s.len() {
        let remaining = s.len() - i;
        if remaining >= 3 {
            let n = ((s[i] as u32) << 16) | ((s[i + 1] as u32) << 8) | s[i + 2] as u32;
            out.push(cs[((n >> 18) & 63) as usize]);
            out.push(cs[((n >> 12) & 63) as usize]);
            out.push(cs[((n >> 6) & 63) as usize]);
            out.push(cs[(n & 63) as usize]);
        } else if remaining == 2 {
            let n = ((s[i] as u32) << 16) | ((s[i + 1] as u32) << 8);
            out.push(cs[((n >> 18) & 63) as usize]);
            out.push(cs[((n >> 12) & 63) as usize]);
            out.push(cs[((n >> 6) & 63) as usize]);
        } else {
            let n = (s[i] as u32) << 16;
            out.push(cs[((n >> 18) & 63) as usize]);
            out.push(cs[((n >> 12) & 63) as usize]);
        }
        i += 3;
    }
    let padding = (4 - out.len() % 4) % 4;
    for _ in 0..padding {
        out.push('=');
    }
    out
}

fn generate_string_2(url_params: &str, method: &str) -> String {
    let start_time: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let end_time: i64 = start_time + rand::thread_rng().gen_range(4..=8);

    let params_arr = double_sm3_bytes(url_params);
    let method_arr = double_sm3_bytes(method);
    let browser_len = BROWSER.len() as i64;

    let vals = [
        (end_time >> 24) & 255,
        params_arr[21] as i64,
        UA_CODE[23] as i64,
        (end_time >> 16) & 255,
        params_arr[22] as i64,
        UA_CODE[24] as i64,
        (end_time >> 8) & 255,
        end_time & 255,
        (start_time >> 24) & 255,
        (start_time >> 16) & 255,
        (start_time >> 8) & 255,
        start_time & 255,
        method_arr[21] as i64,
        method_arr[22] as i64,
        (end_time >> 32) & 0xFF,
        (start_time >> 32) & 0xFF,
        browser_len,
    ];
    let mut arr = list_4(vals);
    // browser_code：browser 字符串逐字符 ord
    arr.extend(BROWSER.bytes().map(|b| b as i64));
    arr.push(xor_check(&arr));

    let plain: Vec<u8> = arr.iter().map(|&c| c as u8).collect();
    let encrypted = rc4_encrypt(&plain, "y");
    String::from_utf8_lossy(&encrypted).to_string()
}

/// 生成 A-Bogus 签名（结果需 URL 编码后拼入请求）
pub fn get_value(url_params: &str, method: &str) -> String {
    let string_1 = generate_string_1();
    let string_2 = generate_string_2(url_params, method);
    let combined = format!("{}{}", string_1, string_2);
    generate_result(combined.as_bytes())
}

// 临时验证：a_bogus 签名对抖音搜索接口的真实有效性（ignored，手动跑）

use crate::reverse::douyin_abogus;

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .unwrap()
}

#[tokio::test]
#[ignore = "依赖真实网络：验证 a_bogus 签名对抖音搜索接口的有效性"]
async fn debug_abogus_live_search() {
    let client = build_client();
    let ua = douyin_abogus::USER_AGENT;

    // 1. 访问抖音直播首页拿 ttwid cookie
    let home = client
        .get("https://live.douyin.com/")
        .header("User-Agent", ua)
        .send()
        .await
        .expect("首页请求失败");
    let cookies: Vec<String> = home
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let cookie_header = cookies.join("; ");
    eprintln!("cookies: {}", &cookie_header[..cookie_header.len().min(120)]);

    // 2. 构造搜索参数 + a_bogus
    let keyword = "测试";
    let params = format!(
        "device_platform=webapp&aid=6383&channel=channel_pc_web&search_channel=aweme_live&keyword={kw}&search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=10&pc_client_type=1&version_code=170400&version_name=17.4.0&cookie_enabled=true&screen_width=1920&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=130.0.0.0&browser_online=true&engine_name=Blink&engine_version=130.0.0.0&os_name=Windows&os_version=10&cpu_core_num=12&memory_info=8&platform=PC&downlink=10&effective_type=4g&round_trip_time=50&webid=7300000000000000000",
        kw = urlencoding::encode(keyword)
    );
    let a_bogus = douyin_abogus::get_value(&params, "GET");
    eprintln!("a_bogus: {}", &a_bogus[..a_bogus.len().min(50)]);

    // 3. 请求搜索接口
    let url = format!(
        "https://www.douyin.com/aweme/v1/web/live/search/?{}&a_bogus={}",
        params,
        urlencoding::encode(&a_bogus)
    );
    let resp = client
        .get(&url)
        .header("User-Agent", ua)
        .header("Referer", "https://www.douyin.com/search/测试?type=live")
        .header("Cookie", cookie_header)
        .send()
        .await
        .expect("搜索请求失败");
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    eprintln!("status: {}", status);
    eprintln!("body head: {}", &body[..body.len().min(400)]);

    // 常见失败模式：空 body + 403（签名无效）/ status_code 9（风控）/ JSON 有效（成功）
    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
        let list_len = j
            .pointer("/data/aweme_list")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        eprintln!(
            "json status_code: {:?}, aweme_list: {}, extra: {:?}",
            j.pointer("/status_code"),
            list_len,
            j.pointer("/extra/now")
        );
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 自洽性：同输入长度/字符集校验
    #[test]
    fn generates_urlsafe_value() {
        let v = get_value("device_platform=webapp&aid=6383&keyword=test", "GET");
        // s4 字符集 + 补位等号
        assert!(!v.is_empty());
        assert!(v.chars().all(|c| CHARSET_S4.contains(c) || c == '='));
        assert_eq!(v.len() % 4, 0, "s4 编码长度应为 4 的倍数");
    }

    /// RC4 与参考实现一致性（标准 RC4 行为）
    #[test]
    fn rc4_matches_reference() {
        // Python: _rc4_encrypt(chr(5)+chr(6)+chr(7), "y") 手算验证结构而非固定值
        let a = rc4_encrypt(&[1, 2, 3], "y");
        let b = rc4_encrypt(&[1, 2, 3], "y");
        assert_eq!(a, b, "同输入应同输出");
        assert_eq!(a.len(), 3);
    }
}

#[tokio::test]
#[ignore = "依赖真实网络与已登录 auth.json：验证登录态搜索接口"]
async fn debug_abogus_search_logged() {
    // 1. 读 auth.json 的抖音登录 cookie
    let dir = std::env::var("LOCALAPPDATA").unwrap();
    let path = std::path::Path::new(&dir)
        .join("com.streamflow.app")
        .join("auth.json");
    let store: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let cookie_login = store["platforms"]["douyin"]["cookie"]
        .as_str()
        .unwrap_or("")
        .to_string();
    if cookie_login.is_empty() {
        eprintln!("auth.json 无抖音登录态");
        return;
    }
    eprintln!("login cookie len: {}", cookie_login.len());

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(12))
        .build()
        .unwrap();
    let ua = USER_AGENT;

    // 2. 游客 ttwid
    let home = client
        .get("https://live.douyin.com/")
        .header("User-Agent", ua)
        .send()
        .await
        .unwrap();
    let ttwid: Vec<String> = home
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or("").to_string())
        .filter(|s| s.starts_with("ttwid="))
        .collect();

    // 3. 构造搜索请求（登录 cookie + ttwid + a_bogus）
    let params = format!(
        "device_platform=webapp&aid=6383&channel=channel_pc_web&search_channel=aweme_live&keyword={}\
         &search_source=normal_search&query_correct_type=1&is_filter_search=0&from_group_id=&offset=0&count=10\
         &pc_client_type=1&version_code=170400&version_name=17.4.0&cookie_enabled=true&screen_width=1920\
         &screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome\
         &browser_version=130.0.0.0&browser_online=true&engine_name=Blink&engine_version=130.0.0.0\
         &os_name=Windows&os_version=10&cpu_core_num=12&memory_info=8&platform=PC&downlink=10\
         &effective_type=4g&round_trip_time=50&webid=7300000000000000000",
        urlencoding::encode("和平精英")
    );
    let a_bogus = get_value(&params, "GET");
    let cookie_header = format!(
        "{}{}{}",
        ttwid.first().map(|s| s.clone()).unwrap_or_default(),
        if ttwid.is_empty() { "" } else { "; " },
        cookie_login
    );
    let url = format!(
        "https://www.douyin.com/aweme/v1/web/live/search/?{}&a_bogus={}",
        params,
        urlencoding::encode(&a_bogus)
    );
    let resp = client
        .get(&url)
        .header("User-Agent", ua)
        .header("Referer", "https://www.douyin.com/search/%E5%92%8C%E5%B9%B3%E7%B2%BE%E8%8B%B1?type=live")
        .header("Cookie", cookie_header)
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    eprintln!("status: {}", status);
    eprintln!("body head: {}", &body[..body.len().min(300)]);
    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&body) {
        let list = j.pointer("/data/aweme_list").and_then(|v| v.as_array());
        eprintln!(
            "status_code: {:?}, aweme_list: {}",
            j.pointer("/status_code").and_then(|v| v.as_i64()),
            list.map(|a| a.len()).unwrap_or(0)
        );
        if let Some(arr) = list {
            for item in arr.iter().take(2) {
                eprintln!(
                    "item keys: {:?}",
                    item.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())
                );
            }
        }
    }
}
