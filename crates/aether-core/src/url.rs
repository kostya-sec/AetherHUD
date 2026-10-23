//! URL 归一化与去重哈希（#10 §8 / #28）。
//!
//! 全应用只有这一份 canonical URL 实现：去重轨道、未来的打开链接与导出都用它。
//! 规则：scheme/host 小写、去默认端口、去 fragment、去跟踪参数、去非根路径的
//! 末尾斜杠、query 按 (key, value) 排序；哈希取 BLAKE3 前 128 bit。

use url::Url;

/// BLAKE3 前 128 bit（16 字节），与 schema 的 entries.url_hash BLOB 一致。
pub const URL_HASH_BYTES: usize = 16;

/// 归一化后的 URL：对外打开用 canonical，去重用 hash。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalUrl {
    /// 归一化后的绝对 URL（小写 scheme/host、稳定 query 顺序）。
    pub canonical: String,
    /// BLAKE3(canonical) 的前 128 bit。
    pub hash: [u8; URL_HASH_BYTES],
    /// 小写 host（视频来源识别等按 host 判断）。
    pub host: String,
}

/// 归一化一个用户输入的 URL；不是 http(s) 绝对 URL 时返回 None。
///
/// 纯文本（含空串、无 scheme 的 example.com/x、mailto: 等）一律视为非 URL，
/// 由调用方走纯文本录入路径。
pub fn normalize(raw: &str) -> Option<CanonicalUrl> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut url = Url::parse(trimmed).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return None;
    }

    // 默认端口：url crate 解析时通常已省略；对显式写出的默认端口再兜底一次。
    let scheme_default_port = match url.scheme() {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    };
    if url.port().is_some() && url.port() == scheme_default_port {
        let _ = url.set_port(None);
    }

    url.set_fragment(None);

    // 非根路径去末尾斜杠（根路径 "/" 保留：WHATWG 序列化下它就是空路径）。
    let path = url.path().to_owned();
    if path.len() > 1 {
        let trimmed_path = path.trim_end_matches('/');
        let normalized_path = if trimmed_path.is_empty() {
            "/"
        } else {
            trimmed_path
        };
        if normalized_path != path {
            url.set_path(normalized_path);
        }
    }

    // query：去跟踪参数 + 按 (key, value) 排序，保证同一资源只有一个 canonical。
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking_param(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        pairs.sort();
        let mut serializer = url.query_pairs_mut();
        serializer.clear();
        serializer.extend_pairs(
            pairs
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        );
    }

    let canonical = url.to_string();
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    Some(CanonicalUrl {
        hash: hash_bytes(canonical.as_bytes()),
        canonical,
        host,
    })
}

/// BLAKE3 前 128 bit；独立导出便于用已知测试向量证明哈希实现。
pub fn hash_bytes(bytes: &[u8]) -> [u8; URL_HASH_BYTES] {
    let digest = blake3::hash(bytes);
    let mut hash = [0u8; URL_HASH_BYTES];
    hash.copy_from_slice(&digest.as_bytes()[..URL_HASH_BYTES]);
    hash
}

/// 跟踪参数（广告/来源归因），对内容身份没有意义。
fn is_tracking_param(key: &str) -> bool {
    const TRACKING_KEYS: &[&str] = &[
        "fbclid",
        "gclid",
        "gbraid",
        "wbraid",
        "msclkid",
        "yclid",
        "igshid",
        "mc_cid",
        "mc_eid",
        "spm",
        "scm",
        "ref_src",
        "ref_url",
        "_hsenc",
        "_hsmi",
        "vero_id",
        "wickedid",
        "oly_anon_id",
        "oly_enc_id",
    ];
    let lower = key.to_ascii_lowercase();
    lower.starts_with("utm_") || TRACKING_KEYS.contains(&lower.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical(raw: &str) -> String {
        normalize(raw)
            .unwrap_or_else(|| panic!("应识别为 URL：{raw}"))
            .canonical
    }

    #[test]
    fn lowercase_scheme_host_and_drop_default_port() {
        assert_eq!(
            canonical("HTTPS://Example.COM:443/Path/"),
            "https://example.com/Path"
        );
        assert_eq!(canonical("http://Example.com:80/a"), "http://example.com/a");
        // 非默认端口保留。
        assert_eq!(
            canonical("http://example.com:8080/a"),
            "http://example.com:8080/a"
        );
    }

    #[test]
    fn drop_fragment_and_trailing_slash_but_keep_root() {
        assert_eq!(
            canonical("https://example.com/page/#section"),
            "https://example.com/page"
        );
        assert_eq!(
            canonical("https://example.com/page///"),
            "https://example.com/page"
        );
        assert_eq!(canonical("https://example.com/"), "https://example.com/");
        assert_eq!(canonical("https://example.com"), "https://example.com/");
    }

    #[test]
    fn drop_tracking_params_and_sort_query_by_key() {
        assert_eq!(
            canonical("https://example.com/a?b=2&utm_source=x&a=1&utm_medium=y&fbclid=z#frag"),
            "https://example.com/a?a=1&b=2"
        );
        assert_eq!(
            canonical("https://example.com/a?b=1&a=2&a=1"),
            "https://example.com/a?a=1&a=2&b=1"
        );
        assert_eq!(
            canonical("https://example.com/a?utm_source=x&utm_campaign=y"),
            "https://example.com/a"
        );
        // 普通参数与跟踪参数混排：只删除跟踪参数。
        assert_eq!(
            canonical("https://example.com/a?spm=abc&page=2"),
            "https://example.com/a?page=2"
        );
    }

    #[test]
    fn normalization_is_idempotent() {
        let once = canonical("https://Example.com/x/?utm_source=t&b=2&a=1#f");
        assert_eq!(canonical(&once), once);
    }

    #[test]
    fn non_http_urls_and_plain_text_are_not_urls() {
        for raw in [
            "",
            "   ",
            "随手记一条：rust 异步",
            "example.com/page",
            "mailto:someone@example.com",
            "ftp://example.com/file",
            "file:///C:/x.txt",
            "javascript:alert(1)",
        ] {
            assert!(normalize(raw).is_none(), "不应识别为 URL：{raw}");
        }
        // 前后空白不影响识别。
        assert!(normalize("  https://example.com/x  ").is_some());
    }

    #[test]
    fn url_hash_is_blake3_first_128_bits() {
        // BLAKE3 官方测试向量：blake3("") 的前 16 字节。
        let expected = [
            0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc,
            0xc9, 0x49,
        ];
        assert_eq!(hash_bytes(b""), expected);
    }

    #[test]
    fn equivalent_urls_share_a_hash_and_distinct_urls_do_not() {
        let a = normalize("https://example.com/path?a=1&b=2").unwrap();
        let b = normalize("HTTPS://EXAMPLE.COM:443/path/?utm_source=x&b=2&a=1#frag").unwrap();
        let c = normalize("https://example.com/path?a=1&b=3").unwrap();
        assert_eq!(a.canonical, b.canonical);
        assert_eq!(a.hash, b.hash);
        assert_ne!(a.hash, c.hash);
        assert_eq!(a.host, "example.com");
    }

    #[test]
    fn percent_encoded_query_is_stable_across_equivalent_spellings() {
        let a = normalize("https://example.com/s?q=%E4%B8%AD%E6%96%87").unwrap();
        let b = normalize("https://example.com/s?q=%e4%b8%ad%e6%96%87").unwrap();
        assert_eq!(a.canonical, b.canonical);
    }
}
