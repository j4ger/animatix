//! The `Applicable` table must agree with what the primitives actually read.
//!
//! `Applicable::Actors(&[…])` rows are the inspector's property list, the plan
//! slot filter, and — since this round — the build's `inapplicable-property`
//! lint. Nothing checked those rows against the `match prop.name.as_str()` arms
//! in the primitives themselves, so a row could omit a type that reads the
//! property and only a false positive in someone's editor would notice. Two were
//! wrong when this test was written: `text_max_width` listed only `Legend` while
//! the text engine treats it as the canonical wrap width (94 shipped scenes),
//! and `font_size` omitted `Legend`, which reads it for its own labels.
//!
//! Scope, stated honestly:
//! - Only `Actors(&[…])` rows are cross-checked — 59 of the table's rows. The capability-shaped
//!   rows (`TextLike`, `SizedActors`, …) need an `ActorCaps` to evaluate, which this test
//!   deliberately does not reconstruct.
//! - A read is recognised as a `"name" =>` match arm or a `== "name"` test inside the primitive's
//!   own file. Reads that live in shared helpers are not attributed to a type, so the test can
//!   under-report, never invent.

use std::collections::BTreeMap;
use std::path::Path;

/// `property name -> the type list its `Actors` row declares`.
fn actor_rows(property_rs: &str) -> BTreeMap<String, Vec<String>> {
    let mut rows = BTreeMap::new();
    for chunk in property_rs.split("PropertyDescriptor::new(").skip(1) {
        let Some(name) = first_quoted(chunk) else {
            continue;
        };
        let Some(start) = chunk.find("Applicable::Actors(&[") else {
            continue;
        };
        let list = &chunk[start + "Applicable::Actors(&[".len()..];
        let Some(end) = list.find("]") else { continue };
        let types = quoted_in(&list[..end]);
        if !types.is_empty() {
            rows.insert(name, types);
        }
    }
    rows
}

/// The first quoted string in a slice — the row's property name, which contains
/// underscores and so cannot share [`quoted_in`]'s identifier filter.
fn first_quoted(text: &str) -> Option<String> {
    let start = text.find('"')?;
    let rest = &text[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn quoted_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let mut j = i + 1;
            let mut word = String::new();
            while j < bytes.len() && bytes[j] != b'"' {
                word.push(bytes[j] as char);
                j += 1;
            }
            if word.chars().all(|c| c.is_ascii_alphabetic()) && !word.is_empty() {
                out.push(word);
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Property names a primitive file matches on: `"name" =>` arms and
/// `== "name"` comparisons (the latter only when the literal is on the left of
/// the operator, which is how the `prop.name == "x"` reads are written too).
fn read_properties(source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let Some(close) = source[i + 1..].find('"') else {
            break;
        };
        let word = &source[i + 1..i + 1 + close];
        i = i + close + 2;
        if word.is_empty() || !word.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            continue;
        }
        let tail = source[i..].trim_start();
        if tail.starts_with("=>") || tail.starts_with("==") {
            names.push(word.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

#[test]
fn bar_chart_gap_is_applicable_to_bar_chart() {
    // The generic test above cannot see this one: `BarChart` has no primitive
    // file reading its properties, because the chart's builder lives in the
    // shared plot builder (`timeline/build/plot.rs`) rather than in
    // `src/primitives/`. `gap` was listed for the layout containers alone while
    // that builder read it as the bar spacing, so `animatix check` reported
    // `unknown-property` for a property that works and documented, and the
    // inspector left it off BarChart's property list.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let property_rs =
        std::fs::read_to_string(manifest.join("../animatix-core/src/property.rs")).expect("table");
    let rows = actor_rows(&property_rs);
    let gap_types = rows
        .get("gap")
        .expect("the table has no `gap` row — the row was renamed or removed");
    assert!(
        gap_types.iter().any(|ty| ty == "BarChart"),
        "`BarChart` reads `gap` as the bar spacing but the row lists {gap_types:?}"
    );

    let plot_rs =
        std::fs::read_to_string(manifest.join("src/timeline/build/plot.rs")).expect("plot builder");
    assert!(
        read_properties(&plot_rs).iter().any(|name| name == "gap"),
        "the plot builder no longer reads `gap` — drop this test with the read"
    );
}

#[test]
fn every_actor_row_covers_the_type_that_reads_it() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let property_rs =
        std::fs::read_to_string(manifest.join("../animatix-core/src/property.rs")).expect("table");
    let rows = actor_rows(&property_rs);
    assert!(
        rows.len() >= 35,
        "expected to parse the Actors-shaped rows, got {} — the parser and the table have drifted",
        rows.len()
    );

    let dir = manifest.join("src/primitives");
    let mut disagreements: Vec<String> = Vec::new();
    let mut audited = 0;
    for entry in std::fs::read_dir(&dir).expect("primitives directory") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("primitive source");
        // A file can declare helper structs before the primitive itself, so
        // scan every `pub struct` and take the one named `…Primitive`.
        let Some(ty) = source
            .split("pub struct ")
            .skip(1)
            .filter_map(|rest| rest.split(|c: char| !c.is_ascii_alphanumeric() && c != '_').next())
            .find_map(|name| name.strip_suffix("Primitive"))
            .map(str::to_string)
        else {
            continue;
        };
        let ty = ty.as_str();
        audited += 1;
        for prop in read_properties(&source) {
            if let Some(types) = rows.get(&prop) {
                if !types.iter().any(|declared| declared == ty) {
                    disagreements.push(format!(
                        "{}: {ty} reads \"{prop}\" but the row lists {:?}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                        types
                    ));
                }
            }
        }
    }

    assert!(
        audited >= 20,
        "only {audited} primitives were audited — the file convention changed"
    );
    assert!(
        disagreements.is_empty(),
        "the applicability table disagrees with the primitives:\n{}",
        disagreements.join("\n")
    );
}
