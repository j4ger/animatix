use animatix_syntax::{highlight::ranges, parser::parse_source_complete};

/// Browser editors use UTF-16 offsets. The coordinate conversion is linear
/// even for non-BMP source and does not change parser-assigned roles.
pub fn highlight_source(source: &str) -> Vec<(usize, usize, &'static str)> {
    let parsed = parse_source_complete(source);
    utf16_ranges(source, ranges(&parsed.tokens, &parsed.occurrences, &parsed.headers))
}

/// Immediate feedback while a semantic pass is debounced.
pub fn highlight_lexical(source: &str) -> Vec<(usize, usize, &'static str)> {
    utf16_ranges(source, ranges(&animatix_syntax::token::tokenize(source), &[], &[]))
}

fn utf16_ranges(
    source: &str,
    mut ranges: Vec<(usize, usize, &'static str)>,
) -> Vec<(usize, usize, &'static str)> {
    let (mut byte, mut units) = (0, 0);
    for (from, to, _) in &mut ranges {
        let end = *to;
        let start = *from;
        *from = units + source[byte..start].encode_utf16().count();
        units = *from + source[start..end].encode_utf16().count();
        byte = end;
        *to = units;
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::highlight_source;

    #[test]
    fn colors_amx_roles_at_utf16_offsets_with_an_incomplete_tail() {
        let source = "#0s\n  label: Text, text: \"中文🎬\"\n#+2b\n  // 下一拍\n  label.opacity =";
        let units: Vec<_> = source.encode_utf16().collect();
        let marked: Vec<_> = highlight_source(source)
            .into_iter()
            .map(|(from, to, role)| (String::from_utf16(&units[from..to]).unwrap(), role))
            .collect();
        for (text, role) in [
            ("#0s", "keyframe"),
            ("Text", "type"),
            ("text", "property"),
            ("\"中文🎬\"", "string"),
            ("#+2b", "keyframe"),
            ("// 下一拍", "comment"),
            ("=", "operator"),
        ] {
            assert!(marked.iter().any(|(s, r)| s == text && *r == role), "{marked:?}");
        }
    }

    #[test]
    fn colors_cue_annotations_separately_from_time_headers() {
        let source = "#voice@intro\n#cue:start\n";
        let marked = highlight_source(source);
        assert!(marked.contains(&(0, 12, "keyframe")));
        assert!(marked.contains(&(13, 23, "cue")));
    }
}
