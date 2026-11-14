//! Pin `docs/properties.md` to the descriptor table.
//!
//! The generated tables and handwritten prose must agree: every canonical
//! descriptor needs a row, and every documented property name must
//! either be a descriptor or one of the explicitly-listed aliases below.
//!
//! The alias list is deliberately explicit rather than derived: a name that is
//! neither a descriptor nor a known alias is a documentation bug (a typo or a
//! removed property), and a descriptor without a row is a missing entry.

use std::collections::BTreeSet;

use animatix_core::property::PROPERTY_DESCRIPTORS;

/// Property names the reference may mention that are not descriptor rows.
///
/// These fall into two groups:
///
/// - **Engine track names** that authors see indirectly (`motion_offset`,
///   `layout_size`, `shape_type`, `text_content`, `placement_mode`,
///   `morph_options`, `stroke_color`, `vector_paths`, `text_paths`,
///   `line_from`, `line_to`, `arc_angles`). They appear in prose and examples,
///   but the public spelling is the descriptor name.
/// - **Filter post-processing knobs** (`blur`, `brightness`, `contrast`,
///   `saturate`, `hue_rotate`, `sepia`): the `Filter` primitive parses these
///   directly and they have no descriptor row because they are never stored on
///   an actor track.
const DOC_ALIASES: &[&str] = &[
    // Engine track names.
    "motion_offset",
    "layout_size",
    "shape_type",
    "text_content",
    "placement_mode",
    "morph_options",
    "stroke_color",
    "vector_paths",
    "text_paths",
    "line_from",
    "line_to",
    "arc_angles",
    // Filter-local properties (see `primitives/filter.rs`).
    "blur",
    "brightness",
    "contrast",
    "saturate",
    "hue_rotate",
    "sepia",
];

/// One documented property name and the line it came from.
struct DocName<'a> {
    name: &'a str,
    line: usize,
}

fn parse_doc(contents: &str) -> Result<Vec<DocName<'_>>, String> {
    let mut names = Vec::new();
    let mut in_property_table = false;

    for (index, raw_line) in contents.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim();

        if line.starts_with("## ") {
            in_property_table = false;
            continue;
        }

        if !line.starts_with('|') {
            in_property_table = false;
            continue;
        }

        let cells: Vec<&str> = line
            .trim_start_matches('|')
            .trim_end_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        let Some(first) = cells.first() else {
            continue;
        };

        if !in_property_table {
            if *first == "Property" {
                in_property_table = true;
            }
            continue;
        }

        if cells
            .iter()
            .all(|cell| !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':'))
        {
            continue;
        }

        if !(first.starts_with('`') && first.ends_with('`') && first.matches('`').count() == 2) {
            return Err(format!(
                "docs/properties.md:{line_no}: property table row's first column is not a \
                 single backticked name: {first:?}"
            ));
        }

        let name = &first[1..first.len() - 1];
        if name.is_empty() {
            return Err(format!(
                "docs/properties.md:{line_no}: property table row has an empty name"
            ));
        }
        names.push(DocName {
            name,
            line: line_no,
        });
    }

    Ok(names)
}

#[test]
fn property_reference_matches_descriptors() {
    let doc = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/properties.md"));
    let names = parse_doc(doc).expect("docs/properties.md parses as property tables");

    let descriptors: BTreeSet<&str> = PROPERTY_DESCRIPTORS.iter().map(|d| d.name).collect();
    let aliases: BTreeSet<&str> = DOC_ALIASES.iter().copied().collect();
    assert!(aliases.len() == DOC_ALIASES.len(), "DOC_ALIASES contains a duplicate entry");

    let mut documented = BTreeSet::new();
    for DocName { name, line } in &names {
        if !documented.insert(*name) {
            panic!("docs/properties.md:{line}: `{name}` is documented more than once");
        }
        assert!(
            descriptors.contains(name) || aliases.contains(name),
            "docs/properties.md:{line}: `{name}` is neither a PROPERTY_DESCRIPTORS row nor a \
             DOC_ALIASES entry"
        );
    }

    let missing: Vec<&str> = descriptors.difference(&documented).copied().collect();
    assert!(
        missing.is_empty(),
        "docs/properties.md is missing descriptor rows for: {missing:?}"
    );
}
