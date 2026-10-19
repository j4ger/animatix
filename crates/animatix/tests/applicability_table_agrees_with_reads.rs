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
//! - `Actors(&[…])` and `ShapeKinds(&[…])` rows are cross-checked — 58 + 4 of the table's rows.
//!   A `ShapeKinds` row declares a shape capability rather than a name list, so its type list is
//!   reconstructed from the catalog (`animatix_std::caps_for_type` / `CATALOG`): every catalog
//!   type whose `caps.shape` is one of the listed kinds. That keeps the coverage these rows had
//!   when they were written as `Actors(&["Ellipse"])`-style name lists. The remaining
//!   capability-shaped rows (`TextLike`, `SizedActors`, …) need an `ActorCaps` predicate
//!   evaluation this test deliberately does not reconstruct.
//! - A read is recognised as a `"name" =>` match arm or a `== "name"` test inside the primitive's
//!   own file. Reads that live in shared helpers are not attributed to a type, so the test can
//!   under-report, never invent.

use std::collections::BTreeMap;
use std::path::Path;

/// `property name -> the type list its `Actors` row declares`.
fn actor_rows() -> BTreeMap<String, Vec<String>> {
    let mut rows = BTreeMap::new();
    for desc in animatix_core::property::PROPERTY_DESCRIPTORS {
        if let animatix_core::caps::Applicable::Actors(actors) = desc.applicable {
            rows.insert(desc.name.to_string(), actors.iter().map(|s| s.to_string()).collect());
        }
    }
    rows
}

/// `property name -> the type list its `ShapeKinds` row declares`, expanded
/// through the catalog.
///
/// A `ShapeKinds(&[ShapeKind::Ellipse])` row means "every actor that *is* an
/// ellipse", not one hand-written name, so the declared list has to be rebuilt
/// from `caps.shape`. Without this the four shape rows would fall out of the
/// cross-check entirely when they were converted from name lists.
fn shape_kind_rows() -> BTreeMap<String, Vec<String>> {
    let mut rows = BTreeMap::new();
    for desc in animatix_core::property::PROPERTY_DESCRIPTORS {
        if let animatix_core::caps::Applicable::ShapeKinds(kinds) = desc.applicable {
            let mut types: Vec<String> = animatix_std::CATALOG
                .iter()
                .filter_map(|info| {
                    let ty = info.static_type_name()?;
                    let caps = animatix_std::caps_from_info(info);
                    caps.shape.is_some_and(|shape| kinds.contains(&shape)).then(|| ty.to_string())
                })
                .collect();
            types.sort();
            rows.insert(desc.name.to_string(), types);
        }
    }
    rows
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
    let rows = actor_rows();
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
    let mut rows = actor_rows();
    let shape_rows = shape_kind_rows();
    assert_eq!(
        shape_rows.len(),
        4,
        "expected four `ShapeKinds` rows (`commands`, `points`, `radius_x`, `radius_y`), got {:?} \
         — a row changed shape and lost this coverage",
        shape_rows.keys().collect::<Vec<_>>()
    );
    for (property, types) in &shape_rows {
        assert!(
            !types.is_empty(),
            "`{property}` is declared for a shape kind no catalog actor has"
        );
    }
    rows.extend(shape_rows);
    assert!(
        rows.len() >= 39,
        "expected to parse the Actors- and ShapeKinds-shaped rows, got {} — the parser and the \
         table have drifted",
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

/// Properties that are genuinely consumed but not by their *name string* in the
/// engine, with the typed field that reads them. This list is the whole point of
/// the test below: adding an entry is an assertion that something reads the
/// property, so a new entry needs a reason, not a shrug.
///
/// Reasons that appear here:
/// - `Applicable::Never`: engine-derived values the renderer writes and reads
///   through a typed lane (`Ascent` / `Descent`), never author-facing.
/// - Read through a **renamed tag**: the binding's `ActorField::Tagged("…")`
///   names a different string than the property, so the reader never mentions
///   the property name (`bounds` → `filter_bounds`, `place` → `callout_place`).
/// - Read through typed storage: `track.text.char_progress`, and the Taffy
///   `constraints.*` the layout builder fills from `min_width` / `min_height` /
///   `max_height`.
/// - Deferred with a diagnostic: `perspective` has no consumer yet, so the
///   build warns `unimplemented-property` when it is authored.
const READ_WITHOUT_A_NAME_STRING: &[(&str, &str)] = &[
    (
        "ascent",
        "Applicable::Never; written and read as the `Ascent` lane by font metrics",
    ),
    (
        "descent",
        "Applicable::Never; written and read as the `Descent` lane by font metrics",
    ),
    ("char_progress", "read as `track.text.char_progress` (typewriter reveal)"),
    (
        "min_width",
        "read as the Taffy `constraints.min_width` the layout builder fills",
    ),
    (
        "min_height",
        "read as the Taffy `constraints.min_height` the layout builder fills",
    ),
    (
        "max_height",
        "read as the Taffy `constraints.max_height` the layout builder fills",
    ),
    ("standoff", "read as `track.geometry.callout_standoff`"),
    ("to_offset", "read as `track.geometry.callout_to_offset`"),
    ("bounds", "binding tag is `filter_bounds`, read as `Tagged(\"filter_bounds\")`"),
    ("place", "binding tag is `callout_place`, read as `Tagged(\"callout_place\")`"),
    (
        "perspective",
        "deferred: focal length for the planned 2.5D perspective tilt (roadmap); no consumer \
         yet, so `build/actor.rs` warns `unimplemented-property` when it is authored",
    ),
];

/// Every bound property must have a reader.
///
/// `every_actor_row_covers_the_type_that_reads_it` checks one direction: a read
/// must be declared in the table. Nothing checked the other direction — that a
/// declared *and bound* property is read at all — which is how `max_lines` and
/// `transition_time` sat in `BINDINGS`, showed up in the inspector, and did
/// nothing for as long as they did.
///
/// A reader here means the property name appears as a string literal anywhere in
/// the engine or text crates outside `property_registry.rs` (which is where the
/// bindings themselves live) — `sample_*_prop("name", …)`, a `"name" =>` arm, or
/// a `== "name"` test all count. Reads that go through a typed lane instead are
/// listed in [`READ_WITHOUT_A_NAME_STRING`].
///
/// Scope, stated honestly: a property whose name is a common word (`value`,
/// `color`, `size`) matches an unrelated literal somewhere and is treated as
/// read. This is a floor that catches distinctly-named dead bindings, not a
/// proof of liveness for every row.
#[test]
fn every_bound_property_has_a_reader() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().expect("crates/ directory");

    let mut sources: Vec<String> = Vec::new();
    for root in [manifest.join("src"), workspace.join("animatix-text/src")] {
        collect_rust_sources(&root, &mut sources);
    }
    assert!(sources.len() > 50, "expected to scan the engine sources, got {}", sources.len());

    let mut unread: Vec<&str> = Vec::new();
    for schema in animatix::timeline::PROPERTY_REGISTRY.iter() {
        let name = schema.name;
        if READ_WITHOUT_A_NAME_STRING.iter().any(|(exempt, _)| *exempt == name) {
            continue;
        }
        let needle = format!("\"{name}\"");
        if !sources.iter().any(|src| src.contains(&needle)) {
            unread.push(name);
        }
    }

    assert!(
        unread.is_empty(),
        "these bound properties have no reader in the engine, so authoring them does nothing:\n  \
         {}\n\
         Either give each one a real consumer, remove its binding, or add it to \
         READ_WITHOUT_A_NAME_STRING with the typed field that reads it.",
        unread.join("\n  ")
    );
}

/// Collect `.rs` sources under `dir`, skipping test modules and the binding table
/// itself (a binding mentioning its own name must not count as a reader).
fn collect_rust_sources(dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("tests") {
                continue;
            }
            collect_rust_sources(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some("property_registry.rs") {
            continue;
        }
        if let Ok(source) = std::fs::read_to_string(&path) {
            out.push(source);
        }
    }
}
