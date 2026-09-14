/// Returns the first `max` characters of `s` as a new String.
/// Never panics on multi-byte boundaries.
pub fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// If `s` has more than `head + tail` characters, returns
/// `first head chars + '…' + last tail chars`. Otherwise returns `s` unchanged.
// Only called from the `video`-gated export status UI; kept (and unit-tested)
// unconditionally so the default build still type-checks and tests it.
#[cfg_attr(not(feature = "video"), allow(dead_code))]
pub fn truncate_middle(s: &str, head: usize, tail: usize) -> String {
    let count = s.chars().count();
    if count > head + tail + 1 {
        let h: String = s.chars().take(head).collect();
        let t: String = s.chars().skip(count - tail).collect();
        format!("{}…{}", h, t)
    } else {
        s.to_string()
    }
}

/// Ellipsize `s` with a trailing `…` so its measured width fits `max_width`.
///
/// `measure` reports the rendered width of a candidate string, which keeps this
/// independent of egui (and therefore unit-testable). Binary search over the
/// prefix length: O(log n) measurements per call. Returns `…` when not even
/// the ellipsis fits.
pub fn elide_to_width(s: &str, max_width: f32, measure: impl Fn(&str) -> f32) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if measure(s) <= max_width {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if measure(&candidate) <= max_width {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let candidate: String = chars[..lo].iter().collect::<String>() + "…";
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit of width per character — stands in for a font measurement.
    fn measure_chars(s: &str) -> f32 {
        s.chars().count() as f32
    }

    #[test]
    fn elide_keeps_text_that_fits() {
        assert_eq!(elide_to_width("short", 10.0, measure_chars), "short");
    }

    #[test]
    fn elide_truncates_to_the_widest_prefix_that_fits() {
        // 6 units of budget: "abc" + "…" == 4 fits, "abcd" + "…" == 5 fits,
        // "abcde" + "…" == 6 fits, "abcdef" + "…" == 7 does not.
        assert_eq!(elide_to_width("abcdefgh", 6.0, measure_chars), "abcde…");
    }

    #[test]
    fn elide_falls_back_to_the_ellipsis_when_nothing_fits() {
        assert_eq!(elide_to_width("abcdefgh", 1.0, measure_chars), "…");
        assert_eq!(elide_to_width("abcdefgh", 0.0, measure_chars), "");
    }

    #[test]
    fn elide_is_utf8_safe() {
        assert_eq!(elide_to_width("中文测试文本", 4.0, measure_chars), "中文测…");
    }

    #[test]
    fn test_truncate_chars_ascii() {
        assert_eq!(truncate_chars("hello", 3), "hel");
    }

    #[test]
    fn test_truncate_chars_multibyte() {
        assert_eq!(truncate_chars("héllo", 3), "hél");
    }

    #[test]
    fn test_truncate_chars_cjk() {
        assert_eq!(truncate_chars("中文测试", 2), "中文");
    }

    #[test]
    fn test_truncate_middle_short() {
        assert_eq!(truncate_middle("hello", 2, 2), "hello");
    }

    #[test]
    fn test_truncate_middle_long() {
        let r = truncate_middle("hello world", 2, 2);
        assert!(r.starts_with("he"));
        assert!(r.ends_with("ld"));
        assert!(r.contains('…'));
    }

    #[test]
    fn test_truncate_middle_multibyte() {
        let r = truncate_middle("中文测试文本", 2, 2);
        assert!(r.starts_with("中文"));
        assert!(r.ends_with("文本"));
    }
}
