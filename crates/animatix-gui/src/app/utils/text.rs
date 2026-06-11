/// Returns the first `max` characters of `s` as a new String.
/// Never panics on multi-byte boundaries.
pub fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// If `s` has more than `head + tail` characters, returns
/// `first head chars + '…' + last tail chars`. Otherwise returns `s` unchanged.
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

#[cfg(test)]
mod tests {
    use super::*;

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
