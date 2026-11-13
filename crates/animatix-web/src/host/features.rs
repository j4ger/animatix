//! Source-level engine feature requirements.
//!
//! A conservative upper bound for source tooling, independent of runtime inputs.
//! Prepared programs evaluate reachable declarations to select on-demand blocks;
//! unknown or extension primitives must produce preparation diagnostics.

use animatix_core::features::FeatureMask;
use animatix_std::catalog::catalog_lookup;
use animatix_syntax::ast::Stmt;
use animatix_syntax::walk::{walk_inline_items, walk_stmts};

/// Union the `required_features` of every actor declared in `stmts`.
pub fn required_features(stmts: &[Stmt]) -> FeatureMask {
    let mut mask = FeatureMask::NONE;
    walk_stmts(stmts, &mut |stmt| {
        let Stmt::ActorDecl { ty, children, .. } = stmt else {
            return;
        };
        mask |= features_for_type(ty);
        walk_inline_items(children, &mut |item| {
            let ty = match item {
                animatix_syntax::ast::InlineItem::Anonymous { ty, .. }
                | animatix_syntax::ast::InlineItem::Labeled { ty, .. } => ty,
                _ => return,
            };
            mask |= features_for_type(ty);
        });
    });
    mask
}

fn features_for_type(ty: &str) -> FeatureMask {
    if let Some(info) = catalog_lookup(ty) {
        return info.required_features;
    }
    // Effect children inside a Filter/Glass scope are built-ins, not unknown
    // primitives; they do not require any optional capability.
    if animatix_std::is_effect(ty) {
        return FeatureMask::NONE;
    }
    FeatureMask::UNKNOWN
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Vec<Stmt> {
        animatix_syntax::parser::parse_source(source).0.expect("source should parse")
    }

    #[test]
    fn plain_scene_has_no_optional_features() {
        assert_eq!(required_features(&parse("a: Rect\nb: Text, text: \"hi\"")), FeatureMask::NONE);
    }

    #[test]
    fn rich_text_primitives_require_rich_text() {
        for ty in ["Code", "Math", "Typst"] {
            let source = format!("a: {ty}");
            assert!(
                required_features(&parse(&source)).contains(FeatureMask::RICH_TEXT),
                "{ty} must require rich text"
            );
        }
    }

    #[test]
    fn image_and_svg_have_distinct_bits() {
        assert!(
            required_features(&parse("a: Image, url: \"x.png\""))
                .contains(FeatureMask::IMAGE_DECODE)
        );
        assert!(required_features(&parse("a: Svg, url: \"x.svg\"")).contains(FeatureMask::SVG));
    }

    #[test]
    fn nested_and_imported_statements_are_scanned() {
        let source = r#"row: Row {
  label: Text, text: "x"
  math: Math
}"#;
        assert!(
            required_features(&parse(source)).contains(FeatureMask::RICH_TEXT),
            "a primitive nested in a keyframe/container must be found"
        );
    }

    #[test]
    fn filter_effect_children_are_not_unknown() {
        let source = r#"panel: Filter {
  soft: Blur, radius: 4
  glow: Bloom, intensity: 0.5
}"#;
        assert_eq!(
            required_features(&parse(source)),
            FeatureMask::NONE,
            "effect children must not be mistaken for unknown actor primitives"
        );
    }

    #[test]
    fn unknown_primitive_sets_unknown_capability_bit() {
        assert!(required_features(&parse("a: CustomExtension")).contains(FeatureMask::UNKNOWN));
    }
}
