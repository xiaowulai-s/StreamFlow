//! 弹幕协议用的最小 protobuf 编解码
//!
//! 抖音（PushFrame/Response/ChatMessage）与快手（SocketMessage/ScWebFeedPush）
//! 只需要 varint 与 length-delimited 两类字段，手写比引入 prost/protoc 更轻。

/// protobuf 字段值（仅支持本项目需要的两种 wire type）
#[derive(Debug, Clone, Copy)]
pub enum Field<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

/// 读取 varint（最多 10 字节，溢出返回 None）
pub fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value: u64 = 0;
    let mut shift = 0;
    while *pos < buf.len() {
        let byte = buf[*pos];
        *pos += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// 解析一帧里的全部字段；wire type 1/5 直接跳过，非法数据停止解析
pub fn parse_fields(buf: &[u8]) -> Vec<(u32, Field<'_>)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < buf.len() {
        let Some(key) = read_varint(buf, &mut pos) else {
            break;
        };
        let field = (key >> 3) as u32;
        let wire = (key & 0x07) as u8;
        match wire {
            0 => {
                let Some(value) = read_varint(buf, &mut pos) else {
                    break;
                };
                out.push((field, Field::Varint(value)));
            }
            2 => {
                let Some(len) = read_varint(buf, &mut pos) else {
                    break;
                };
                let len = len as usize;
                if pos + len > buf.len() {
                    break;
                }
                out.push((field, Field::Bytes(&buf[pos..pos + len])));
                pos += len;
            }
            1 => pos += 8,
            5 => pos += 4,
            _ => break,
        }
    }
    out
}

/// 解析第一个指定字段的 bytes 值
pub fn first_bytes<'a>(fields: &[(u32, Field<'a>)], field: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|(f, v)| match (f, v) {
        (f, Field::Bytes(b)) if *f == field => Some(*b),
        _ => None,
    })
}

/// 解析第一个指定字段的 varint 值
pub fn first_varint(fields: &[(u32, Field<'_>)], field: u32) -> Option<u64> {
    fields.iter().find_map(|(f, v)| match (f, v) {
        (f, Field::Varint(n)) if *f == field => Some(*n),
        _ => None,
    })
}

/// 解析第一个指定字段的 UTF-8 字符串
pub fn first_string(fields: &[(u32, Field<'_>)], field: u32) -> Option<String> {
    first_bytes(fields, field).map(|b| String::from_utf8_lossy(b).to_string())
}

pub fn varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(10);
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// 编码 varint 字段（wire type 0）
pub fn field_varint(field: u32, value: u64) -> Vec<u8> {
    let mut out = varint(((field as u64) << 3) | 0);
    out.extend_from_slice(&varint(value));
    out
}

/// 编码 length-delimited 字段（wire type 2）
pub fn field_bytes(field: u32, data: &[u8]) -> Vec<u8> {
    let mut out = varint(((field as u64) << 3) | 2);
    out.extend_from_slice(&varint(data.len() as u64));
    out.extend_from_slice(data);
    out
}

pub fn field_string(field: u32, value: &str) -> Vec<u8> {
    field_bytes(field, value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for value in [0u64, 1, 127, 128, 300, 1794380000000, u64::MAX] {
            let bytes = varint(value);
            let mut pos = 0;
            assert_eq!(read_varint(&bytes, &mut pos), Some(value));
            assert_eq!(pos, bytes.len());
        }
    }

    #[test]
    fn field_roundtrip() {
        let mut buf = field_varint(1, 6383);
        buf.extend_from_slice(&field_string(7, "hb"));
        let fields = parse_fields(&buf);
        assert_eq!(first_varint(&fields, 1), Some(6383));
        assert_eq!(first_string(&fields, 7).as_deref(), Some("hb"));
    }
}
