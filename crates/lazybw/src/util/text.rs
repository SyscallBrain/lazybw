//! Text measurement and presentation helpers.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Truncates to `width` columns, appending `…` when anything was removed.
#[must_use]
pub fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let w = grapheme.width();
        if used + w > width.saturating_sub(1) {
            break;
        }
        out.push_str(grapheme);
        used += w;
    }
    out.push('…');
    out
}

/// Pads with spaces to exactly `width` columns, truncating if needed.
#[must_use]
pub fn pad(text: &str, width: usize) -> String {
    let truncated = truncate(text, width);
    let used = truncated.width();
    let mut out = truncated;
    for _ in used..width {
        out.push(' ');
    }
    out
}

/// Right-aligns in `width` columns.
#[must_use]
pub fn pad_left(text: &str, width: usize) -> String {
    let truncated = truncate(text, width);
    let used = truncated.width();
    let mut out = String::new();
    for _ in used..width {
        out.push(' ');
    }
    out.push_str(&truncated);
    out
}

/// A masked secret: one bullet per character, capped at `max` bullets.
#[must_use]
pub fn masked(len: usize, max: usize) -> String {
    "•".repeat(len.clamp(1, max).min(len.max(1)))
}

/// Groups a TOTP code: 6 digits → `482 913`, 8 digits → `4829 1374`.
#[must_use]
pub fn group_code(code: &str, digits: usize) -> String {
    match digits {
        6 if code.len() >= 6 => format!("{} {}", &code[..3], &code[3..6]),
        8 if code.len() >= 8 => format!("{} {}", &code[..4], &code[4..8]),
        _ => code.to_owned(),
    }
}

/// Splits by graphemes so cursor movement never breaks a character.
#[must_use]
pub fn graphemes(text: &str) -> Vec<&str> {
    text.graphemes(true).collect()
}

/// Column width of a string.
#[must_use]
pub fn width(text: &str) -> usize {
    text.width()
}

/// Width of one character, treating control characters as zero.
#[must_use]
pub fn char_width(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

/// True when the string looks like a URL.
#[must_use]
pub fn looks_like_url(value: &str) -> bool {
    url::Url::parse(value).is_ok()
}

/// A count of items for messages: `1 item`, `3 items`.
#[must_use]
pub fn items(count: usize) -> String {
    if count == 1 { "1 item".to_owned() } else { format!("{count} items") }
}

/// Removes a leading scheme for display.
#[must_use]
pub fn strip_scheme(value: &str) -> &str {
    value.strip_prefix("https://").or_else(|| value.strip_prefix("http://")).unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_uses_ellipsis() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 6), "hello…");
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn padding_is_exact() {
        assert_eq!(pad("hi", 5), "hi   ");
        assert_eq!(pad_left("hi", 5), "   hi");
        assert_eq!(pad("hello world", 5).width(), 5);
    }

    #[test]
    fn masks_and_groups() {
        assert_eq!(masked(10, 20), "••••••••••");
        assert_eq!(masked(0, 20), "•");
        assert_eq!(group_code("482913", 6), "482 913");
        assert_eq!(group_code("48291374", 8), "4829 1374");
        assert_eq!(group_code("ABCDE", 5), "ABCDE");
    }

    #[test]
    fn item_counts_agree_with_the_number() {
        assert_eq!(items(0), "0 items");
        assert_eq!(items(1), "1 item");
        assert_eq!(items(248), "248 items");
    }

    #[test]
    fn urls() {
        assert!(looks_like_url("https://example.com"));
        assert!(!looks_like_url("example"));
        assert_eq!(strip_scheme("https://example.com/x"), "example.com/x");
    }
}
