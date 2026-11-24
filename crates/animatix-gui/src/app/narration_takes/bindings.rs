use std::collections::{HashMap, HashSet};

use animatix_syntax::narration::paragraphs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Binding {
    pub scene: Option<String>,
    pub selector: String,
    pub text: String,
    pub id: String,
}

/// Keep only one-to-one matches. Repeated text can match inside the same
/// bounded interval, but a whole document of identical lines has no identity
/// evidence once its source digest changes.
pub(super) fn reconcile(source: &str, previous: &[Binding]) -> Vec<Binding> {
    let mut current: Vec<_> = paragraphs(source)
        .into_iter()
        .map(|paragraph| Binding {
            selector: paragraph.id(),
            scene: paragraph.scene,
            text: paragraph.text,
            id: String::new(),
        })
        .collect();
    let scenes: HashSet<_> = current.iter().map(|binding| binding.scene.clone()).collect();
    for scene in scenes {
        let old: Vec<_> = previous.iter().filter(|binding| binding.scene == scene).collect();
        let mut new: Vec<_> = current.iter_mut().filter(|binding| binding.scene == scene).collect();
        match_scene(&old, &mut new);
    }
    for binding in &mut current {
        if binding.id.is_empty() {
            binding.id = Uuid::new_v4().simple().to_string();
        }
    }
    current
}

fn match_scene(old: &[&Binding], new: &mut [&mut Binding]) {
    let old_text = text_positions(old.iter().map(|binding| binding.text.as_str()));
    let new_text = text_positions(new.iter().map(|binding| binding.text.as_str()));
    let mut matches: Vec<_> = new_text
        .iter()
        .filter_map(|(text, positions)| {
            let old = old_text.get(text)?;
            (old.len() == 1 && positions.len() == 1).then(|| (old[0], positions[0]))
        })
        .collect();
    matches.sort_unstable();
    for &(before, after) in &matches {
        new[after].id.clone_from(&old[before].id);
    }
    for pair in matches.windows(2) {
        let [(old_start, new_start), (old_end, new_end)] = pair else {
            continue;
        };
        if *new_end <= *new_start {
            continue;
        }
        let old_gap = &old[old_start + 1..*old_end];
        let new_gap = &mut new[new_start + 1..*new_end];
        if old_gap.len() == 1 && new_gap.len() == 1 && new_gap[0].id.is_empty() {
            let binding = old_gap[0];
            if !new_text.contains_key(binding.text.as_str()) || binding.text == new_gap[0].text {
                new_gap[0].id.clone_from(&binding.id);
            }
        }
    }
}

fn text_positions<'a>(texts: impl Iterator<Item = &'a str>) -> HashMap<String, Vec<usize>> {
    let mut positions = HashMap::<String, Vec<usize>>::new();
    for (index, text) in texts.enumerate() {
        positions.entry(text.to_owned()).or_default().push(index);
    }
    positions
}

/// Reuse an ID for a localized edit entirely inside one paragraph when the
/// surrounding ordered script stays the same. Loads use the stricter matcher.
pub(super) fn edit(before: &str, after: &str, previous: &[Binding]) -> Vec<Binding> {
    if before == after {
        return previous.to_vec();
    }
    let mut current = reconcile(after, previous);
    match_moves(previous, &mut current);
    if current.len() != previous.len() {
        return current;
    }
    let prefix = before.bytes().zip(after.bytes()).take_while(|(a, b)| a == b).count();
    let suffix = before.as_bytes()[prefix..]
        .iter()
        .rev()
        .zip(after.as_bytes()[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old = paragraphs(before);
    let new = paragraphs(after);
    if let Some(index) = old.iter().zip(&new).position(|(old, new)| {
        old.scene == new.scene
            && old.span.start <= prefix
            && new.span.start <= prefix
            && before.len() - suffix <= old.span.end
            && after.len() - suffix <= new.span.end
            && old.text != new.text
    }) {
        let same_surroundings = previous
            .iter()
            .zip(&current)
            .enumerate()
            .all(|(i, (a, b))| i == index || (a.scene == b.scene && a.text == b.text));
        if same_surroundings {
            for (a, b) in previous.iter().zip(&mut current) {
                b.id.clone_from(&a.id);
            }
        }
    } else if previous
        .iter()
        .zip(&current)
        .all(|(a, b)| a.scene == b.scene && a.text == b.text)
    {
        for (a, b) in previous.iter().zip(&mut current) {
            b.id.clone_from(&a.id);
        }
    }
    current
}

fn match_moves(previous: &[Binding], current: &mut [Binding]) {
    let existing: HashSet<_> = current.iter().map(|binding| binding.id.clone()).collect();
    let old_ids: HashSet<_> = previous.iter().map(|binding| &binding.id).collect();
    let old_text = text_positions(previous.iter().map(|binding| binding.text.as_str()));
    let new_text = text_positions(current.iter().map(|binding| binding.text.as_str()));
    for binding in current {
        let Some([index]) = old_text.get(&binding.text).map(Vec::as_slice) else {
            continue;
        };
        let old = &previous[*index];
        if new_text[&binding.text].len() == 1
            && old.scene != binding.scene
            && !existing.contains(&old.id)
            && !old_ids.contains(&binding.id)
        {
            binding.id.clone_from(&old.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_edit_keeps_the_take_on_its_paragraph() {
        let before = "/// Hello.\n\n/// Same.\n\n/// Same.\n";
        let after = "/// Hello again.\n\n/// Same.\n\n/// Same.\n";
        let previous = reconcile(before, &[]);
        let updated = edit(before, after, &previous);
        assert!(previous.iter().zip(updated).all(|(a, b)| a.id == b.id));
    }

    #[test]
    fn external_duplicate_removal_does_not_select_a_take() {
        let previous = reconcile("/// Same.\n\n/// Same.\n", &[]);
        let updated = reconcile("/// Same.\n", &previous);
        assert!(previous.iter().all(|old| old.id != updated[0].id));
    }
}
