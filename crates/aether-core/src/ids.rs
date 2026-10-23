//! 对外条目标识 ULID（#10 §2：entries.uid 存 ULID 作对外标识）。
//!
//! 48 bit Unix 毫秒时间戳 + 80 bit 随机数，按 Crockford Base32 编码为 26 字符。
//! 时间戳由调用方传入（适配器注入时钟），随机数用进程内 fastrand。

/// ULID 字母表（Crockford Base32，去除 I/L/O/U）。
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// ULID 文本长度。
pub const ULID_LEN: usize = 26;

/// 生成一个 ULID；now_ms 来自注入的时钟，保证测试可预测时间部分。
pub fn new_ulid(now_ms: i64) -> String {
    let mut random = [0u8; 10];
    let random_bits = fastrand::u128(..);
    for (index, byte) in random.iter_mut().enumerate() {
        // 取低 80 bit，逐字节填充。
        *byte = (random_bits >> (index * 8)) as u8;
    }
    encode_ulid(now_ms, random)
}

/// 从时间戳与 80 bit 随机数编码 ULID（纯函数，便于用已知向量验证）。
pub fn encode_ulid(now_ms: i64, random: [u8; 10]) -> String {
    let timestamp = (now_ms.max(0) as u64) & 0xFFFF_FFFF_FFFF; // 48 bit
    let mut bytes = [0u8; 16];
    bytes[..6].copy_from_slice(&timestamp.to_be_bytes()[2..]);
    bytes[6..].copy_from_slice(&random);

    let mut value = u128::from_be_bytes(bytes);
    let mut encoded = [0u8; ULID_LEN];
    for slot in encoded.iter_mut().rev() {
        *slot = ALPHABET[(value & 31) as usize];
        value >>= 5;
    }
    String::from_utf8(encoded.to_vec()).expect("ULID 字母表是合法 UTF-8")
}

/// 是否为结构合法的 ULID（长度 + 字母表，大小写不敏感）。
pub fn is_ulid(value: &str) -> bool {
    value.len() == ULID_LEN
        && value
            .bytes()
            .all(|byte| ALPHABET.contains(&byte.to_ascii_uppercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_spec_example_timestamp() {
        // ULID 规范示例 01ARZ3NDEKTSV4RRFFQ69G5FAV 的时间部分是 1469922850259；
        // 随机部分置零后应为 26 字符（前 10 位时间、后 16 位全 0）。
        assert_eq!(
            encode_ulid(1_469_922_850_259, [0u8; 10]),
            "01ARZ3NDEK0000000000000000"
        );
    }

    #[test]
    fn generated_ulids_are_26_chars_unique_and_ordered_by_time_prefix() {
        let earlier = new_ulid(1_761_000_000_000);
        let later = new_ulid(1_761_000_000_001);
        assert_eq!(earlier.len(), ULID_LEN);
        assert_eq!(later.len(), ULID_LEN);
        assert!(is_ulid(&earlier));
        assert!(is_ulid(&later));
        assert_ne!(earlier, later);
        // 同毫秒内也应不同（80 bit 随机）。
        assert_ne!(new_ulid(1_761_000_000_000), new_ulid(1_761_000_000_000));
        // 时间戳变大 → 时间部分字典序靠后（ULID 可按字符串排序）。
        assert!(later[..10] > earlier[..10]);
    }

    #[test]
    fn rejects_malformed_ulids() {
        assert!(!is_ulid(""));
        assert!(!is_ulid("01ARZ3NDEKTSV4RRFFQ69G5FA"));
        assert!(!is_ulid("01ARZ3NDEKTSV4RRFFQ69G5FAI")); // I 不在字母表
        assert!(is_ulid("01arz3ndektsv4rrffq69g5fav")); // 小写允许
    }
}
