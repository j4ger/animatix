//! Bundled stroke icons for the `icon:` build-time property on `Path` actors.
//!
//! This is a plain data table — name → SVG path data — that the engine expands
//! into `Path` geometry (see `animatix::timeline::path_data::parse_svg_path_data`,
//! the single path-data parser; there is deliberately no second one here). It is
//! *not* a primitive and has no catalog row: `icon:` is sugar for an authored
//! `commands:` list, so tracing the stroke with `draw-in` works exactly as it
//! does for any hand-written path.
//!
//! ## Source & license
//!
//! Every entry is copied **verbatim** from a
//! [Lucide](https://github.com/lucide-icons/lucide) icon. Lucide is
//! distributed under the ISC license:
//!
//! > Copyright (c) Lucide Contributors (<https://github.com/lucide-icons/lucide>)
//! >
//! > Permission to use, copy, modify, and/or distribute this software for any
//! > purpose with or without fee is hereby granted, provided that the above
//! > copyright notice and this permission notice appear in all copies.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
//! > WITH REGARD TO THIS SOFTWARE.
//!
//! The engine bundles these for playback only and never re-hosts the upstream
//! repository; the attribution above is the notice the license requires.
//!
//! ## Coordinate grid
//!
//! Lucide icons are drawn on a **24 × 24** grid with the origin at the
//! top-left, `fill: none`, `stroke-width: 2`, and round line caps/joins. The
//! path data is stored verbatim, so a `Path` actor consuming an icon places the
//! icon's top-left corner at its own geometry origin and it extends down and to
//! the right toward `(24, 24)` in the actor's local space. `Path` geometry is
//! **not** auto-centered by bounding box (the primitive renders the parsed
//! `custom_path` directly), so position the icon with `at` / `anchor`. Note that
//! a render probe found `scale` does not visibly enlarge `Path` geometry, so the
//! marks render near their native ~24 px size; verify sizing against the engine
//! before relying on `scale` here.
//!
//! ## Why only straight-segment icons
//!
//! This set is intentionally restricted to icons whose upstream path data uses
//! only the commands the reused parser approximates *exactly*: `M/m`, `L/l`,
//! `H/h`, `V/v`, `C/c`, `Q/q`, `S/s`, `T/t`, `Z/z`. Lucide icons that rely on SVG
//! elliptical arcs (`A`/`a` — e.g. `circle`, `heart`, `star`, `play`) are **not**
//! bundled, because `parse_svg_path_data` still approximates arcs as a straight
//! chord, which would render those marks as faceted polygons rather than clean
//! curves. Adding them is a parser task (implement arc → Bézier), not a data task.
//!
//! [Lucide]: https://lucide.dev

/// One bundled stroke icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrokeIcon {
    /// Canonical name authored in `icon: "<name>"`.
    pub name: &'static str,
    /// Path-data subpaths, one entry per upstream `<path d="…">` element, stored
    /// verbatim. A `Path` actor with several subpaths parses each one into its
    /// own command run so a leading relative moveto (`m…`) resolves against the
    /// path origin, matching the separate `<path>` elements in the source SVG.
    pub path_data: &'static [&'static str],
    /// Extra search synonyms, for tooling and closest-name suggestions. Not part
    /// of the rendered geometry.
    pub keywords: &'static [&'static str],
}

impl StrokeIcon {
    /// Const constructor for [`STROKE_ICONS`].
    pub const fn new(
        name: &'static str,
        path_data: &'static [&'static str],
        keywords: &'static [&'static str],
    ) -> Self {
        Self {
            name,
            path_data,
            keywords,
        }
    }
}

/// The bundled set, sorted by `name` (the `table_invariants` test pins it).
pub static STROKE_ICONS: &[StrokeIcon] = &[
    StrokeIcon::new("arrow-down", &["M12 5v14", "m19 12-7 7-7-7"], &["down", "download"]),
    StrokeIcon::new("arrow-down-left", &["M17 7 7 17", "M17 17H7V7"], &["diagonal", "southwest"]),
    StrokeIcon::new("arrow-down-right", &["m7 7 10 10", "M17 7v10H7"], &["diagonal", "southeast"]),
    StrokeIcon::new("arrow-left", &["m12 19-7-7 7-7", "M19 12H5"], &["back", "left", "previous"]),
    StrokeIcon::new(
        "arrow-left-to-line",
        &["M3 19V5", "m13 6-6 6 6 6", "M7 12h14"],
        &["back", "reset", "tab"],
    ),
    StrokeIcon::new("arrow-right", &["M5 12h14", "m12 5 7 7-7 7"], &["next", "forward", "right"]),
    StrokeIcon::new(
        "arrow-right-to-line",
        &["M17 12H3", "m11 18 6-6-6-6", "M21 5v14"],
        &["forward", "enter", "tab"],
    ),
    StrokeIcon::new("arrow-up", &["m5 12 7-7 7 7", "M12 19V5"], &["up", "upload", "top"]),
    StrokeIcon::new("arrow-up-left", &["M7 17V7h10", "M17 17 7 7"], &["diagonal", "northwest"]),
    StrokeIcon::new(
        "arrow-up-right",
        &["M7 7h10v10", "M7 17 17 7"],
        &["diagonal", "northeast", "external"],
    ),
    StrokeIcon::new("check", &["M20 6 9 17l-5-5"], &["tick", "done", "ok", "yes"]),
    StrokeIcon::new("chevron-down", &["m6 9 6 6 6-6"], &["down", "expand", "caret"]),
    StrokeIcon::new("chevron-left", &["m15 18-6-6 6-6"], &["back", "left", "caret"]),
    StrokeIcon::new("chevron-right", &["m9 18 6-6-6-6"], &["next", "forward", "caret"]),
    StrokeIcon::new("chevron-up", &["m18 15-6-6-6 6"], &["up", "collapse", "caret"]),
    StrokeIcon::new("code", &["m16 18 6-6-6-6", "m8 6-6 6 6 6"], &["brackets", "source"]),
    StrokeIcon::new("menu", &["M4 5h16", "M4 12h16", "M4 19h16"], &["hamburger", "bars"]),
    StrokeIcon::new("minus", &["M5 12h14"], &["subtract", "remove", "collapse"]),
    StrokeIcon::new(
        "move-horizontal",
        &["m18 8 4 4-4 4", "M2 12h20", "m6 8-4 4 4 4"],
        &["left", "right", "resize"],
    ),
    StrokeIcon::new(
        "move-vertical",
        &["M12 2v20", "m8 18 4 4 4-4", "m8 6 4-4 4 4"],
        &["up", "down", "resize"],
    ),
    StrokeIcon::new("plus", &["M5 12h14", "M12 5v14"], &["add", "new", "expand"]),
    StrokeIcon::new("slash", &["M22 2 2 22"], &["divide", "escape", "backslash"]),
    StrokeIcon::new(
        "trending-down",
        &["M16 17h6v-6", "m22 17-8.5-8.5-5 5L2 7"],
        &["chart", "decline", "finance"],
    ),
    StrokeIcon::new(
        "trending-up",
        &["M16 7h6v6", "m22 7-8.5 8.5-5-5L2 17"],
        &["chart", "growth", "finance"],
    ),
    StrokeIcon::new("x", &["M18 6 6 18", "m6 6 12 12"], &["close", "cancel", "cross"]),
];

/// The icon registered under `name`, if this bundle has one.
pub fn stroke_icon(name: &str) -> Option<&'static StrokeIcon> {
    STROKE_ICONS.iter().find(|icon| icon.name == name)
}

/// Every bundled icon name, in the table's (sorted) order.
pub fn stroke_icon_names() -> &'static [&'static str] {
    use std::sync::LazyLock;
    static NAMES: LazyLock<Vec<&'static str>> =
        LazyLock::new(|| STROKE_ICONS.iter().map(|icon| icon.name).collect());
    &NAMES
}

/// The `path_data` for `name`, when it exists — a convenience for the engine
/// so it does not have to reach through the struct to read the geometry.
pub fn stroke_icon_path_data(name: &str) -> Option<&'static [&'static str]> {
    stroke_icon(name).map(|icon| icon.path_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Commands `parse_svg_path_data` handles exactly (arcs deliberately
    /// excluded — see the module doc).
    const SUPPORTED_COMMANDS: &[char] = &[
        'M', 'm', 'L', 'l', 'H', 'h', 'V', 'v', 'C', 'c', 'Q', 'q', 'S', 's', 'T', 't', 'Z', 'z',
    ];

    fn command_letters(d: &str) -> Vec<char> {
        d.chars().filter(|c| c.is_ascii_alphabetic()).collect()
    }

    #[test]
    fn table_invariants() {
        // Names sorted and unique.
        let mut prev: Option<&str> = None;
        for icon in STROKE_ICONS {
            if let Some(prev) = prev {
                assert!(
                    prev < icon.name,
                    "STROKE_ICONS must be sorted and unique; `{prev}` precedes `{}`",
                    icon.name
                );
            }
            prev = Some(icon.name);
        }

        for icon in STROKE_ICONS {
            assert!(!icon.path_data.is_empty(), "`{}` has no path data", icon.name);
            for d in icon.path_data {
                assert!(!d.trim().is_empty(), "`{}` has an empty subpath", icon.name);
                // Only letters the reused parser handles exactly (no `A`/`a`).
                for letter in command_letters(d) {
                    assert!(
                        SUPPORTED_COMMANDS.contains(&letter),
                        "`{}` uses unsupported command `{letter}`",
                        icon.name
                    );
                }
                // Every non-letter run must be a valid number once the SVG
                // tokenizer splits on signs / second dots; this is the same rule
                // the engine relies on, so a violation means silent `0.0` drops.
                for tok in tokenize_reference(d).into_iter().filter(|t| !is_command(t)) {
                    assert!(
                        tok.parse::<f32>().is_ok(),
                        "`{}` produced an unparseable numeric token `{tok}` from `{d}`",
                        icon.name
                    );
                }
            }
        }
    }

    #[test]
    fn lookups_agree_with_table() {
        assert_eq!(stroke_icon_names().len(), STROKE_ICONS.len());
        for name in stroke_icon_names() {
            assert_eq!(stroke_icon(name).map(|i| i.name), Some(*name));
        }
        assert!(stroke_icon("not-an-icon").is_none());
        assert!(stroke_icon_path_data("check").is_some());
    }

    fn is_command(tok: &str) -> bool {
        tok.chars().all(|c| c.is_ascii_alphabetic()) && tok.len() == 1
    }

    /// A copy of the engine tokenizer's number-splitting rules, kept here so the
    /// core table proves its own data round-trips without depending on the
    /// engine crate.
    fn tokenize_reference(d: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        for ch in d.chars() {
            if ch.is_ascii_alphabetic() {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                tokens.push(ch.to_string());
            } else if ch == '-' || ch == '+' {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
                current.push(ch);
            } else if ch == '.' {
                if current.contains('.') {
                    tokens.push(std::mem::take(&mut current));
                }
                current.push(ch);
            } else if ch.is_ascii_digit() {
                current.push(ch);
            } else if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            tokens.push(current);
        }
        tokens
    }
}
