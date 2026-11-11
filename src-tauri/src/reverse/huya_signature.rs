//! 虎牙签名算法
//! 基于 yt-dlp HuyaLiveIE 的 antiCode 重算逻辑，已对真实 CDN 验证（HTTP 200 拉流成功）
//!
//! 算法：
//! 1. fm 为 base64 编码，解码后取 '_' 前缀（服务端下发的签名盐）
//! 2. ss     = md5_hex(seqid | ctype | t)
//! 3. wsSecret = md5_hex(fm_prefix_u_streamname_ss_wstime)
//! 4. u      = uid 位运算混淆：u1 | (u2 >> 24) | (u3 << 8)
//! 5. seqid  = int((wsTime + 随机秒) * 1000) + uid

/// 最小 base64 解码（标准字母表，兼容 URL-safe 与填充）
pub fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut rev = [255u8; 256];
    for (i, &c) in TABLE.iter().enumerate() {
        rev[c as usize] = i as u8;
    }

    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;

    for c in input.bytes() {
        if c == b'=' {
            break;
        }
        let v = match c {
            b'-' => 62,
            b'_' => 63,
            _ => {
                let v = rev[c as usize];
                if v == 255 {
                    return None;
                }
                v
            }
        };
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// 最小百分号解码（处理 %3D 之类，大写十六进制）
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 解析 query 串（保留原始值，不重复解码）
pub fn parse_query(qs: &str) -> Vec<(String, String)> {
    qs.split('&')
        .filter(|s| !s.is_empty())
        .map(|s| match s.split_once('=') {
            Some((k, v)) => (k.to_string(), v.to_string()),
            None => (s.to_string(), String::new()),
        })
        .collect()
}

fn md5_hex(s: &str) -> String {
    hex::encode(md5::compute(s.as_bytes()).0)
}

/// 生成虎牙流地址（HLS/FLV 通用）
///
/// * `base_url`      - sHlsUrl / sFlvUrl
/// * `suffix`        - sHlsUrlSuffix / sFlvUrlSuffix
/// * `stream_name`   - sStreamName
/// * `anti_code`     - sHlsAntiCode / sFlvAntiCode（原始 query 串）
/// * `presenter_uid` - lPresenterUid
/// * `need_sign`     - 是否需要重算签名（screenType=0 且 liveSourceType ∈ {0,8,13}）
pub fn build_stream_url(
    base_url: &str,
    suffix: &str,
    stream_name: &str,
    anti_code: &str,
    presenter_uid: i64,
    need_sign: bool,
) -> Option<String> {
    let anti_code = anti_code.replace("&amp;", "&");
    let mut params = parse_query(&anti_code);

    let get = |k: &str, params: &[(String, String)]| -> Option<String> {
        params
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
    };

    if need_sign {
        let ws_time = get("wsTime", &params)?;
        let ws_time_int = i64::from_str_radix(&ws_time, 16).ok()?;

        // ct = int(wsTime,16) + 随机小数秒（纳秒部分充当随机源）
        let frac = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .subsec_nanos() as f64
            / 1e9;
        let ct = ws_time_int as f64 + frac;

        // uid 选择：流名以前缀 uid 开头时改用随机 uid
        let uid = if !stream_name.starts_with(&presenter_uid.to_string()) {
            presenter_uid
        } else {
            ((ct % 1e7) * 1e6 % 4294967295.0) as i64
        };

        // u = u1 | (u2 >> 24) | (u3 << 8)（u64 域位运算）
        let uid_u64 = uid as u64;
        let u1 = uid_u64 & 0xffffffff00000000;
        let u2 = uid_u64 & 0xffffffff;
        let u3 = uid_u64 & 0xffffff;
        let u = u1 | (u2 >> 24) | (u3 << 8);

        let seqid = (ct * 1000.0) as i64 + uid;
        let uuid = ((ct % 1e7) * 1e6 % 4294967295.0) as i64;
        let t = "100";
        let ctype = get("ctype", &params).unwrap_or_else(|| "huya_live".to_string());

        // fm: base64 解码后取 '_' 前缀
        let fm_enc = percent_decode(&get("fm", &params)?);
        let fm_decoded = String::from_utf8(base64_decode(&fm_enc)?).ok()?;
        let fm_prefix = fm_decoded.split('_').next().unwrap_or("").to_string();

        let ss = md5_hex(&format!("{}|{}|{}", seqid, ctype, t));
        let ws_secret = md5_hex(&format!(
            "{}_{}_{}_{}_{}",
            fm_prefix, u, stream_name, ss, ws_time
        ));

        // wsSecret 原位替换（CDN 对参数顺序敏感，Python dict 替换亦保留原位置）
        let mut replaced = false;
        for (k, v) in params.iter_mut() {
            if k == "wsSecret" {
                *v = ws_secret.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            params.push(("wsSecret".to_string(), ws_secret));
        }
        params.push(("u".to_string(), u.to_string()));
        params.push(("seqid".to_string(), seqid.to_string()));
        params.push(("ver".to_string(), "1".to_string()));
        params.push(("uuid".to_string(), uuid.to_string()));
        params.push(("t".to_string(), t.to_string()));
    }

    let query: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    Some(format!(
        "{}/{}.{}?{}",
        base_url,
        stream_name,
        suffix,
        query.join("&")
    ))
}
