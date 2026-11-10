//! 斗鱼签名算法
//! 通过 eval 捕获房间页混淆函数 ub98484234 解密后的明文逻辑移植，
//! 已与真实混淆 JS 输出逐字节比对一致，并经 getH5Play 实测（error=0）。
//!
//! 算法：
//! 1. md5(rid + uuid + ts + key) —— key 为页面下发的 vdwdae325w_64we（如 "220120261110"）
//! 2. md5 结果按 8 字符切成 4 个小端 u32
//! 3. XTEA 加密 2 个块（32 轮，delta=0x9e3779b9，key 固定）
//! 4. 后置位混淆（异或/旋转/加减）
//! 5. 输出 LE hex 作为 sign，返回 "v={key}&did={uuid}&tt={ts}&sign={sign}"

/// 左旋转（n 可为 0，安全）
fn rotl(x: u32, n: u32) -> u32 {
    x.rotate_left(n % 32)
}

/// 右旋转（n 可为 0，安全）
fn rotr(x: u32, n: u32) -> u32 {
    x.rotate_right(n % 32)
}

/// 计算斗鱼 H5 播放签名，返回可直接用于 POST 表单的 query 串
pub fn sign(rid: &str, uuid: &str, ts: &str, key: &str) -> String {
    const K2: [u32; 4] = [0x7c2717b7, 0x610ebae5, 0x00471c00, 0x2a287081];

    // 1. md5(rid + uuid + ts + key)
    let cb = format!("{}{}{}{}", rid, uuid, ts, key);
    let rb = hex::encode(md5::compute(cb.as_bytes()).0);

    // 2. 按 8 字符切成 4 个小端 u32
    let mut re = [0u32; 4];
    for i in 0..4 {
        let mut bytes = [0u8; 4];
        for j in 0..4 {
            bytes[j] = u8::from_str_radix(&rb[i * 8 + j * 2..i * 8 + j * 2 + 2], 16)
                .unwrap_or(0);
        }
        re[i] = u32::from_le_bytes(bytes);
    }

    // 3. XTEA 加密（2 块 × 32 轮）
    let delta: u32 = 0x9e3779b9;
    for i in 0..2usize {
        let mut v0 = re[i * 2];
        let mut v1 = re[i * 2 + 1];
        let mut sum: u32 = 0;
        for _ in 0..32 {
            sum = sum.wrapping_add(delta);
            v0 = v0.wrapping_add(
                ((v1 << 4).wrapping_add(K2[0]))
                    ^ v1.wrapping_add(sum)
                    ^ ((v1 >> 5).wrapping_add(K2[1])),
            );
            v1 = v1.wrapping_add(
                ((v0 << 4).wrapping_add(K2[2]))
                    ^ v0.wrapping_add(sum)
                    ^ ((v0 >> 5).wrapping_add(K2[3])),
            );
        }
        re[i * 2] = v0;
        re[i * 2 + 1] = v1;
    }

    // 4. 后置位混淆（顺序与源码一致）
    re[0] ^= K2[0];
    re[0] = re[0].wrapping_add(K2[2]);
    re[0] ^= K2[2];

    re[1] = rotr(re[1], K2[1] % 16);
    re[1] = re[1].wrapping_sub(K2[3]);
    re[1] = re[1].wrapping_sub(K2[1]);
    re[1] = rotl(re[1], K2[3] % 16);

    re[2] = rotl(re[2], K2[0] % 16);
    re[2] = rotr(re[2], K2[2] % 16);
    re[2] = rotr(re[2], K2[0] % 16);
    re[2] = rotl(re[2], K2[2] % 16);
    re[2] = rotr(re[2], K2[2] % 16);

    re[3] = re[3].wrapping_sub(K2[1]);
    re[3] = re[3].wrapping_sub(K2[3]);
    re[3] = re[3].wrapping_sub(K2[1]);
    re[3] = re[3].wrapping_add(K2[3]);

    re[0] = re[0].wrapping_sub(K2[0]);
    re[0] = re[0].wrapping_sub(K2[2]);
    re[0] = rotr(re[0], K2[2] % 16);

    re[1] = rotl(re[1], K2[1] % 16);
    re[1] = rotr(re[1], K2[3] % 16);
    re[1] = rotl(re[1], K2[3] % 16);
    re[1] = re[1].wrapping_sub(K2[3]);

    re[2] ^= K2[0];
    re[2] ^= K2[2];
    re[2] = re[2].wrapping_sub(K2[2]);

    re[3] ^= K2[1];
    re[3] = re[3].wrapping_sub(K2[3]);
    re[3] ^= K2[3];
    re[3] = rotr(re[3], K2[3] % 16);

    // 5. 每字 4 字节小端 hex
    let mut hex_out = String::with_capacity(32);
    for w in re.iter() {
        for j in 0..4usize {
            hex_out.push_str(&format!("{:02x}", (w >> (j * 8)) & 0xff));
        }
    }

    format!("v={}&did={}&tt={}&sign={}", key, uuid, ts, hex_out)
}
