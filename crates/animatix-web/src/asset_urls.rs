//! Literal asset URL discovery shared by the wasm player and native tests.
//!
//! `Image`/`Svg` actors can appear at any nesting depth: inside scenes,
//! components, sequence/always/conditional/loop blocks, keyframes, and
//! container `children` blocks. The statement walk below delegates body
//! recursion to [`animatix_syntax::walk`] so new body-bearing statements are
//! covered automatically; only inline container items need the extra walker.

use animatix_syntax::ast::{Expr, InlineItem, Property, Stmt};
use animatix_syntax::parser::parse_source;
use animatix_syntax::walk::{walk_inline_items, walk_stmts};

/// Literal `url` properties on `Image`/`Svg` actors in `stmts`, in declaration
/// order and deduplicated by first occurrence.
pub fn collect_asset_urls(stmts: &[Stmt]) -> Vec<String> {
    let mut urls = Vec::new();
    walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl {
            ty,
            props,
            children,
            ..
        } = stmt
        {
            push_actor_url(ty, props, &mut urls);
            // `walk_stmts` only descends into statement bodies; container
            // children are an inline-item tree of their own.
            walk_inline_items(children, &mut |item| {
                if let InlineItem::Anonymous { ty, props, .. }
                | InlineItem::Labeled { ty, props, .. } = item
                {
                    push_actor_url(ty, props, &mut urls);
                }
            });
        }
    });
    urls
}

/// Parse `source` and return its literal asset URLs. Parse errors yield no
/// URLs: `load_source` reports them with full context, and a source that does
/// not parse has no asset declarations to fetch.
pub fn collect_asset_urls_from_source(source: &str) -> Vec<String> {
    let (ast, _) = parse_source(source);
    ast.as_deref().map(collect_asset_urls).unwrap_or_default()
}

fn push_actor_url(ty: &str, props: &[Property], urls: &mut Vec<String>) {
    if !matches!(ty, "Image" | "Svg") {
        return;
    }
    let Some(prop) = props.iter().find(|prop| prop.name == "url") else {
        return;
    };
    let Expr::Str(url) = &prop.value else {
        return;
    };
    if !url.is_empty() && !urls.iter().any(|seen| seen == url) {
        urls.push(url.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(source: &str) -> Vec<String> {
        let (ast, errors) = parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        collect_asset_urls(&ast.expect("AST"))
    }

    #[test]
    fn collects_urls_from_every_scene() {
        let source = r#"
# One
hero: Image, url: "hero.png", at: (0, 0)

# Two
detail: Svg, url: "detail.svg", at: (0, 0)
"#;
        assert_eq!(urls(source), ["hero.png", "detail.svg"]);
    }

    #[test]
    fn collects_container_children_at_any_depth() {
        let source = r#"
row: Row {
    avatar: Image, url: "avatar.png"
    inner: Col {
        logo: Svg, url: "logo.svg"
    }
}
"#;
        assert_eq!(urls(source), ["avatar.png", "logo.svg"]);
    }

    #[test]
    fn collects_component_bodies() {
        let source = r#"
pub component Card(icon: Str = "card.png") {
    badge: Image, url: "badge.png"
    nested: Row {
        glyph: Svg, url: "glyph.svg"
    }
}
"#;
        assert_eq!(urls(source), ["badge.png", "glyph.svg"]);
    }

    #[test]
    fn collects_keyframes_and_nested_blocks() {
        let source = r#"
# SceneOne
#0s
first: Image, url: "first.png"
sequence {
    second: Svg, url: "second.svg"
}
always {
    third: Image, url: "third.png"
}
#1s
if true {
    fourth: Image, url: "fourth.png"
}
for item in {1, 2} {
    looped: Svg, url: "looped.svg"
}
"#;
        assert_eq!(
            urls(source),
            [
                "first.png",
                "second.svg",
                "third.png",
                "fourth.png",
                "looped.svg"
            ]
        );
    }

    #[test]
    fn dedupes_by_first_occurrence() {
        let source = r#"
# One
a: Image, url: "shared.png"
b: Svg, url: "shared.png"
c: Image, url: "other.png"
row: Row {
    d: Image, url: "shared.png"
}
"#;
        assert_eq!(urls(source), ["shared.png", "other.png"]);
    }

    #[test]
    fn ignores_dynamic_and_non_asset_urls() {
        let source = r#"
img: Image, url: dynamic_url
tile: Rect, url: "not-an-asset.png"
svg: Svg, url: ""
"#;
        assert!(urls(source).is_empty());
    }
}
