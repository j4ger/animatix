//! Keep source-authored cue bindings attached through a localized script edit.

use std::ops::Range;

use animatix_syntax::narration::{AnchorEdge, AnchorKey, decode_anchor, encode_anchor, paragraphs};

use super::{anchor_for_phrase, phrase_ranges, targets};

#[derive(Clone)]
pub(crate) struct AnchorChange {
    pub scene: Option<String>,
    pub before: AnchorKey,
    pub before_text: String,
    pub after: AnchorKey,
}

pub(super) struct Rebase {
    pub anchors: Vec<AnchorChange>,
    pub edits: Vec<(Range<usize>, String)>,
}

pub(super) fn after_script_edit(
    before: &str,
    after: &str,
    history: &[Vec<AnchorChange>],
) -> Option<Rebase> {
    let (old_range, new_range) = changed_ranges(before, after)?;
    if !script_line_at(before, old_range.start) || !script_line_at(after, new_range.start) {
        return None;
    }
    let old = paragraphs(before);
    let new = paragraphs(after);
    if old.len() != new.len() {
        return None;
    }
    let edited = old.iter().position(|paragraph| within(&old_range, &paragraph.span))?;
    let revised = &new[edited];
    if !within(&new_range, &revised.span)
        || old
            .iter()
            .zip(&new)
            .enumerate()
            .any(|(i, (old, new))| old.scene != new.scene || (i != edited && old.text != new.text))
    {
        return None;
    }

    let mut result = Rebase {
        anchors: Vec::new(),
        edits: Vec::new(),
    };
    for (before, after) in old.iter().zip(&new).filter(|(old, new)| old.id() != new.id()) {
        let whole = |paragraph| {
            let marker = encode_anchor(&anchor_for_phrase(paragraph, None, AnchorEdge::Start)?)?;
            decode_anchor(&marker)
        };
        result.anchors.push(AnchorChange {
            scene: before.scene.clone(),
            before: whole(before)?,
            before_text: before.text.clone(),
            after: whole(after)?,
        });
    }

    if result.anchors.is_empty() || !after.contains("__narration_") {
        return Some(result);
    }

    let (targets, _) = targets::inspect_source(after);
    for target in targets {
        let (Some(marker), Some(span)) = (target.marker, target.marker_span) else {
            continue;
        };
        let Some(key) = decode_anchor(&marker) else {
            continue;
        };
        let find_paragraph = |key: &AnchorKey| {
            old.iter().position(|paragraph| {
                paragraph.scene == target.scene && key.matches_paragraph(paragraph)
            })
        };
        let Some(index) = find_paragraph(&key).or_else(|| {
            let mut paragraph_key = &key;
            for edit in history {
                if let Some(change) = edit.iter().find(|change| {
                    change.scene == target.scene && change.before.same_paragraph(paragraph_key)
                }) {
                    paragraph_key = &change.after;
                }
            }
            find_paragraph(paragraph_key)
        }) else {
            continue;
        };
        let next = &new[index];
        if key.matches_paragraph(next) {
            continue;
        }
        let phrase = if key.normalized_phrase().is_some() {
            let matches = key.phrase_range(&old[index].text).map_or_else(
                || {
                    key.repair_ranges(&next.text)
                        .into_iter()
                        .filter(|range| {
                            let phrase = &next.text[range.clone()];
                            !(phrase.starts_with(word_char)
                                && next.text[..range.start].ends_with(word_char)
                                || phrase.ends_with(word_char)
                                    && next.text[range.end..].starts_with(word_char))
                        })
                        .collect()
                },
                |range| phrase_matches(&old[index].text, range, &next.text),
            );
            let [unique] = matches.as_slice() else {
                continue;
            };
            Some(unique.clone())
        } else {
            None
        };
        let Some(updated) =
            anchor_for_phrase(next, phrase, key.edge()).and_then(|a| encode_anchor(&a))
        else {
            continue;
        };
        let Some(next_key) = decode_anchor(&updated) else {
            continue;
        };
        result.anchors.push(AnchorChange {
            scene: target.scene,
            before: key,
            before_text: old[index].text.clone(),
            after: next_key,
        });
        result.edits.push((span, updated));
    }
    result.edits.sort_by_key(|(range, _)| range.start);
    Some(result)
}

fn within(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

fn phrase_matches(before: &str, range: Range<usize>, after: &str) -> Vec<Range<usize>> {
    let original = &before[range.clone()];
    let left_word_edge =
        original.starts_with(word_char) && !before[..range.start].ends_with(word_char);
    let right_word_edge =
        original.ends_with(word_char) && !before[range.end..].starts_with(word_char);
    phrase_ranges(after, original)
        .into_iter()
        .filter(|found| {
            (!left_word_edge || !after[..found.start].ends_with(word_char))
                && (!right_word_edge || !after[found.end..].starts_with(word_char))
        })
        .collect()
}

fn word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn script_line_at(source: &str, at: usize) -> bool {
    let start = source[..at].rfind('\n').map_or(0, |index| index + 1);
    source[start..].trim_start_matches([' ', '\t']).starts_with("///")
}

fn changed_ranges(before: &str, after: &str) -> Option<(Range<usize>, Range<usize>)> {
    if before == after {
        return None;
    }
    let start = before
        .chars()
        .zip(after.chars())
        .take_while(|(before, after)| before == after)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    let suffix = before[start..]
        .chars()
        .rev()
        .zip(after[start..].chars().rev())
        .take_while(|(before, after)| before == after)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    Some((start..before.len() - suffix, start..after.len() - suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::narration::NarrationAnchor;

    #[test]
    fn keeps_a_unique_phrase_and_offset_through_an_edit() {
        let before = "Look at the card.";
        let marker = encode_anchor(&NarrationAnchor {
            paragraph: before.into(),
            paragraph_occurrence: 1,
            phrase: Some("card".into()),
            phrase_occurrence: 1,
            edge: AnchorEdge::End,
        })
        .unwrap();
        let source =
            format!("# Intro\n/// {before}\n#narration@{marker} + 250ms\nbox.opacity = 1\n");
        let mut revised = source.replace(before, "Look closely at the card.");
        let result = after_script_edit(&source, &revised, &[]).unwrap();
        let [(span, replacement)] = result.edits.as_slice() else {
            panic!("expected one anchor update")
        };
        revised.replace_range(span.clone(), replacement);
        assert!(revised.contains(&format!("#narration@{replacement} + 250ms")));
        assert_eq!(decode_anchor(replacement).unwrap().edge(), AnchorEdge::End);
        for text in [
            "Look at the image.",
            "Look at the card, then at the card.",
            "Look at the cardboard.",
        ] {
            let revised = source.replace(before, text);
            assert!(after_script_edit(&source, &revised, &[]).unwrap().edits.is_empty());
        }
        let draft = source.replace(before, "Look here.");
        let first_edit = after_script_edit(&source, &draft, &[]).unwrap();
        assert!(first_edit.edits.is_empty());
        let finished = draft.replace("Look here.", "Please hold the card.");
        assert_eq!(
            after_script_edit(&draft, &finished, &[first_edit.anchors]).unwrap().edits.len(),
            1
        );
    }
}
