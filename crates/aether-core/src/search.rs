//! FTS5 查询构造（#10 §9 / ADR-0002）。
//!
//! 用户输入永远是**纯文本**，不是 FTS5 语法：按空白与中英文边界切段，每段加双引号
//! （段内引号双写），段间 AND；少于 3 个 Unicode 字符走 LIKE 兜底。标签名搜索是
//! 独立路径（tags → entry_tags），不进这里。
//!
//! LIKE 兜底说明：#10 §9 要求转义 % 与 _。SQLite 的 LIKE 只有在带 ESCAPE 子句时
//! 才能把反斜杠当转义符，所以 Like 计划里的模式已加反斜杠，调用方 SQL 必须写成
//! `LIKE ? ESCAPE '\'`——这比「不加 ESCAPE」更严格地保证了用户输入的字面量语义。
//!
//! 已知限制（#10 §9）：<3 判定按整串查询；多词查询里 1~2 字的单段（如「Rust 异步」的
//! 「异步」）在 trigram 下无法命中，属引擎限制，完整搜索体验在 #30 处理。

/// 少于 3 个 Unicode 字符时 FTS5 trigram 无法命中，走 LIKE 兜底。
pub const MIN_MATCH_CHARS: usize = 3;

/// 一次检索应该怎么执行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchPlan {
    /// fts_entries MATCH ?：已切段、逐段加引号、AND 连接。
    Match(String),
    /// LIKE ? 兜底：模式已转义 % 与 _，SQL 侧须带 ESCAPE '\\'。
    Like(String),
}

/// 构造检索计划；空白查询返回 None（调用方不检索）。
pub fn plan(query: &str) -> Option<SearchPlan> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().count() < MIN_MATCH_CHARS {
        return Some(SearchPlan::Like(format!("%{}%", escape_like(trimmed))));
    }
    let segments = split_segments(trimmed);
    if segments.is_empty() {
        return Some(SearchPlan::Like(format!("%{}%", escape_like(trimmed))));
    }
    let expression = segments
        .iter()
        .map(|segment| format!("\"{}\"", segment.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ");
    Some(SearchPlan::Match(expression))
}

/// LIKE 兜底中转义通配符：SQL 必须以 ESCAPE '\\' 执行，否者 % 与 _ 会变成通配符。
fn escape_like(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// 按空白、标点与中文/非中文边界切段（trigram 不含空格语义，必须切段）。
fn split_segments(query: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut current_is_cjk: Option<bool> = None;

    for ch in query.chars() {
        if is_separator(ch) {
            flush(&mut current, &mut segments);
            current_is_cjk = None;
            continue;
        }
        let is_cjk = is_cjk(ch);
        if current_is_cjk != Some(is_cjk) {
            flush(&mut current, &mut segments);
            current_is_cjk = Some(is_cjk);
        }
        current.push(ch);
    }
    flush(&mut current, &mut segments);
    segments
}

fn flush(current: &mut String, segments: &mut Vec<String>) {
    if !current.is_empty() {
        segments.push(std::mem::take(current));
    }
}

fn is_separator(ch: char) -> bool {
    // 双引号留在段内，按 #10 §9 在引用时双写（FTS5 语法注入的主要入口）。
    if ch.is_whitespace() || (ch.is_ascii_punctuation() && ch != '"') {
        return true;
    }
    matches!(
        ch,
        '，' | '。'
            | '、'
            | '；'
            | '：'
            | '？'
            | '！'
            | '“'
            | '”'
            | '‘'
            | '’'
            | '（'
            | '）'
            | '《'
            | '》'
            | '〈'
            | '〉'
            | '【'
            | '】'
            | '「'
            | '」'
            | '『'
            | '』'
            | '…'
            | '—'
            | '·'
            | '～'
    )
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3400..=0x4DBF   // CJK 扩展 A
        | 0x4E00..=0x9FFF // CJK 基本区
        | 0xF900..=0xFAFF // CJK 兼容表意
        | 0x3040..=0x30FF // 日文假名（同样无空格分词）
        | 0xAC00..=0xD7AF // 韩文音节
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_queries_have_no_plan() {
        assert_eq!(plan(""), None);
        assert_eq!(plan("   "), None);
    }

    #[test]
    fn short_queries_fall_back_to_like() {
        assert_eq!(plan("ab"), Some(SearchPlan::Like("%ab%".into())));
        assert_eq!(plan("中"), Some(SearchPlan::Like("%中%".into())));
        // 前后空白不改变字符数判定。
        assert_eq!(plan("  ab  "), Some(SearchPlan::Like("%ab%".into())));
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        // 2 个字符 → LIKE；% 与 _ 必须按字面量处理。
        assert_eq!(plan("5%"), Some(SearchPlan::Like("%5\\%%".into())));
        assert_eq!(plan("a_"), Some(SearchPlan::Like("%a\\_%".into())));
    }

    #[test]
    fn long_queries_become_quoted_and_anded_segments() {
        assert_eq!(plan("rust"), Some(SearchPlan::Match("\"rust\"".into())));
        assert_eq!(
            plan("rust async"),
            Some(SearchPlan::Match("\"rust\" AND \"async\"".into()))
        );
        assert_eq!(
            plan("rust异步"),
            Some(SearchPlan::Match("\"rust\" AND \"异步\"".into()))
        );
        assert_eq!(
            plan("中文搜索"),
            Some(SearchPlan::Match("\"中文搜索\"".into()))
        );
        assert_eq!(
            plan("rust，异步"),
            Some(SearchPlan::Match("\"rust\" AND \"异步\"".into()))
        );
    }

    #[test]
    fn fts_syntax_injection_is_quoted_away() {
        // 布尔关键字被当成普通词，不改变语义。
        assert_eq!(
            plan("rust OR python"),
            Some(SearchPlan::Match(
                "\"rust\" AND \"OR\" AND \"python\"".into()
            ))
        );
        assert_eq!(
            plan("rust NOT python"),
            Some(SearchPlan::Match(
                "\"rust\" AND \"NOT\" AND \"python\"".into()
            ))
        );
        // 引号被双写进同一个短语。
        assert_eq!(plan("a\"b"), Some(SearchPlan::Match("\"a\"\"b\"".into())));
        // 前缀、列过滤、括号等 FTS5 语法字符都留在引号内。
        for raw in ["rust*", "title:rust", "NEAR(rust python)", "-rust", "^rust"] {
            let Some(SearchPlan::Match(expression)) = plan(raw) else {
                panic!("{raw} 应走 MATCH");
            };
            assert!(expression.starts_with('"') && expression.ends_with('"'));
            assert!(
                !expression.contains(":("),
                "注入字符逃出了引号：{expression}"
            );
        }
    }
}
