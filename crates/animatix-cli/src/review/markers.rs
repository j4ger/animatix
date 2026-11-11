use animatix_analyzer::Analyzer;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WaiverKind {
    AllowOverlap,
    AllowOcclusion,
    AllowOverflow,
    AllowRest,
    Ignore,
}

impl WaiverKind {
    pub(crate) fn silences(self, code: &str) -> bool {
        match self {
            WaiverKind::AllowOverlap => code == "overlap.undeclared",
            WaiverKind::AllowOcclusion => code == "text.occluded",
            WaiverKind::AllowOverflow => matches!(code, "actor.clipped" | "text.overflow"),
            WaiverKind::AllowRest => matches!(code, "rest.ink-floor" | "rest.cast-erased"),
            WaiverKind::Ignore => matches!(
                code,
                "overlap.explained"
                    | "overlap.undeclared"
                    | "text.occluded"
                    | "actor.clipped"
                    | "text.overflow"
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Waiver {
    pub kind: WaiverKind,
    pub label: String,
    pub reason: String,
    pub line: usize,
    pub raw: String,
}

#[derive(Clone, Debug)]
pub(crate) struct MarkerIssue {
    pub line: usize,
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MarkerSet {
    pub waivers: Vec<Waiver>,
    pub notes: Vec<(usize, String)>,
    pub invalid: Vec<MarkerIssue>,
}

pub(crate) const NON_WAIVABLE: &[&str] = &[
    "build.error",
    "build.never-revealed",
    "frame.blank",
    "text.occluded-full",
    "baseline.mismatch",
    "layout.marker-invalid",
];

pub(crate) fn parse_markers(source: &str) -> MarkerSet {
    let mut set = MarkerSet::default();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            continue;
        }

        let comment = trimmed.trim_start_matches('/').trim_start();

        if let Some(rest) = comment.strip_prefix("layout:") {
            // Every kind is hyphenated (`allow-*`), so the separator must be the
            // whole " - " token rather than a bare '-'.
            let rest = rest.trim();
            let Some((kind_str, reason)) = rest.split_once(" - ") else {
                set.invalid.push(MarkerIssue {
                    line: line_num,
                    message: "Invalid marker syntax: expected '// layout: <kind> - <reason>'"
                        .to_string(),
                });
                continue;
            };
            let kind_str = kind_str.trim();
            let reason = reason.trim();

            if reason.is_empty() {
                set.invalid.push(MarkerIssue {
                    line: line_num,
                    message: "Invalid marker syntax: empty reason".to_string(),
                });
                continue;
            }

            let kind = match kind_str {
                "allow-overlap" => WaiverKind::AllowOverlap,
                "allow-occlusion" => WaiverKind::AllowOcclusion,
                "allow-overflow" => WaiverKind::AllowOverflow,
                "allow-rest" => WaiverKind::AllowRest,
                "ignore" => WaiverKind::Ignore,
                _ => {
                    set.invalid.push(MarkerIssue {
                        line: line_num,
                        message: format!("Unknown marker kind: {}", kind_str),
                    });
                    continue;
                },
            };

            set.waivers.push(Waiver {
                kind,
                label: String::new(),
                reason: reason.to_string(),
                line: line_num,
                raw: trimmed.to_string(),
            });
        } else if let Some(rest) = comment.strip_prefix("rest-check: exempt") {
            let rest = rest.trim();
            let reason = rest.strip_prefix('-').unwrap_or(rest).trim();

            if reason.is_empty() {
                set.invalid.push(MarkerIssue {
                    line: line_num,
                    message: "Invalid old marker syntax: empty reason".to_string(),
                });
                continue;
            }

            set.waivers.push(Waiver {
                kind: WaiverKind::AllowRest,
                label: String::new(),
                reason: reason.to_string(),
                line: line_num,
                raw: trimmed.to_string(),
            });
        } else if let Some(reason) = comment.strip_prefix("house-style:") {
            set.notes.push((line_num, reason.trim().to_string()));
        }
    }
    set
}

pub(crate) fn bind_markers(markers: &mut MarkerSet, labels: &[String], analyzer: &Analyzer) {
    let mut declaration_map = BTreeMap::new();
    for label in labels {
        if let Some((start_line_0based, _, _, _)) = analyzer.declaration_range(label) {
            declaration_map.insert(label.clone(), start_line_0based + 1);
        }
    }

    for w in &mut markers.waivers {
        if let Some(label) = find_nearest_label(w.line, labels, &declaration_map) {
            w.label = label;
        } else {
            markers.invalid.push(MarkerIssue {
                line: w.line,
                message: format!("Binding failed: marker at line {} could not be bound to an actor within 5 lines", w.line),
            });
        }
    }
}

pub(crate) fn find_nearest_label(
    marker_line_1based: usize,
    labels: &[String],
    declaration_map: &BTreeMap<String, usize>,
) -> Option<String> {
    let mut best_label = None;
    let mut min_dist = usize::MAX;

    for label in labels {
        let Some(&start_line_1based) = declaration_map.get(label) else {
            continue;
        };
        if start_line_1based >= marker_line_1based && (start_line_1based - marker_line_1based) <= 5
        {
            let dist = start_line_1based - marker_line_1based;
            if dist < min_dist {
                min_dist = dist;
                best_label = Some(label.clone());
            }
        }
    }
    best_label
}

pub(crate) fn waiver_for<'a>(
    set: &'a MarkerSet,
    label: &str,
    ancestors: &[String],
    code: &str,
) -> Option<&'a Waiver> {
    if NON_WAIVABLE.contains(&code) {
        return None;
    }

    // 1. Try label self
    let self_candidates: Vec<&Waiver> = set
        .waivers
        .iter()
        .filter(|w| w.label == label && w.kind.silences(code))
        .collect();

    if let Some(w) = self_candidates.into_iter().max_by_key(|w| w.line) {
        return Some(w);
    }

    // 2. Try ancestors in order
    for ancestor in ancestors {
        let ancestor_candidates: Vec<&Waiver> = set
            .waivers
            .iter()
            .filter(|w| w.label == *ancestor && w.kind.silences(code))
            .collect();

        if let Some(w) = ancestor_candidates.into_iter().max_by_key(|w| w.line) {
            return Some(w);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_markers() {
        let source = concat!(
            "// layout: allow-overlap - reason 1\n",
            "// rest-check: exempt - reason 2\n",
            "// rest-check: exempt - \n",
            "// house-style: some explanation\n",
            "// layout: unknown - reason 3\n",
            "// layout: allow-overlap - \n",
        );
        let set = parse_markers(source);
        assert_eq!(set.waivers.len(), 2);
        assert_eq!(set.notes.len(), 1);
        assert_eq!(set.invalid.len(), 3);
    }

    /// Regression: every kind is hyphenated (`allow-*`), so a bare '-' split
    /// used to turn `allow-overlap` into the unknown kind `allow`.
    #[test]
    fn test_all_waiver_kinds_parse_with_hyphenated_names() {
        let source = concat!(
            "// layout: allow-overlap - r\n",
            "// layout: allow-occlusion - r\n",
            "// layout: allow-overflow - r\n",
            "// layout: allow-rest - r\n",
            "// layout: ignore - r\n",
        );
        let set = parse_markers(source);
        assert!(set.invalid.is_empty(), "unexpected invalid markers: {:?}", set.invalid);
        let kinds: Vec<WaiverKind> = set.waivers.iter().map(|w| w.kind).collect();
        assert_eq!(
            kinds,
            vec![
                WaiverKind::AllowOverlap,
                WaiverKind::AllowOcclusion,
                WaiverKind::AllowOverflow,
                WaiverKind::AllowRest,
                WaiverKind::Ignore,
            ]
        );
        assert!(set.waivers.iter().all(|w| w.reason == "r"));
    }

    #[test]
    fn test_crlf_source_parses() {
        let source = "// layout: allow-overlap - reason\r\n// house-style: note\r\n";
        let set = parse_markers(source);
        assert_eq!(set.waivers.len(), 1);
        assert_eq!(set.notes.len(), 1);
        assert!(set.invalid.is_empty());
    }

    #[test]
    fn test_silences() {
        assert!(WaiverKind::AllowOverlap.silences("overlap.undeclared"));
        assert!(!WaiverKind::AllowOverlap.silences("text.occluded"));
        assert!(WaiverKind::AllowOverflow.silences("actor.clipped"));
        assert!(WaiverKind::AllowOverflow.silences("text.overflow"));
        assert!(!WaiverKind::AllowOverflow.silences("overlap.undeclared"));
    }

    #[test]
    fn test_non_waivable() {
        let set = MarkerSet::default();
        assert!(waiver_for(&set, "test", &[], "build.error").is_none());
    }

    #[test]
    fn test_inheritance_priority() {
        let mut set = MarkerSet::default();
        set.waivers.push(Waiver {
            kind: WaiverKind::AllowOverlap,
            label: "child".to_string(),
            reason: "reason".to_string(),
            line: 1,
            raw: "".to_string(),
        });
        set.waivers.push(Waiver {
            kind: WaiverKind::AllowOverlap,
            label: "parent".to_string(),
            reason: "reason".to_string(),
            line: 10,
            raw: "".to_string(),
        });

        let waiver = waiver_for(&set, "child", &["parent".to_string()], "overlap.undeclared");
        assert!(waiver.is_some());
        assert_eq!(waiver.unwrap().line, 1); // Should be child's (line 1), not parent's (line 10)
    }

    #[test]
    fn test_nearest_label() {
        let mut decls = BTreeMap::new();
        decls.insert("a".to_string(), 2); // 0-based index 1 -> 1-based index 2
        decls.insert("b".to_string(), 6); // 0-based index 5 -> 1-based index 6

        // Marker at 1, nearest is 'a' (dist 1)
        assert_eq!(
            find_nearest_label(1, &["a".to_string(), "b".to_string()], &decls),
            Some("a".to_string())
        );

        // Marker at 6, nearest is 'b' (dist 0)
        assert_eq!(
            find_nearest_label(6, &["a".to_string(), "b".to_string()], &decls),
            Some("b".to_string())
        );

        // Marker at 7, too far
        assert_eq!(find_nearest_label(7, &["a".to_string(), "b".to_string()], &decls), None);
    }
}
