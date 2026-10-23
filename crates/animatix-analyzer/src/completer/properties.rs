use super::*;

pub(super) fn property_completions(
    symbols: &SymbolTable,
    actor_type: Option<&str>,
    manifest: &ExtensionManifest,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    if let Some(ty) = actor_type {
        // Effects are not primitives: offer their declared parameters plus the
        // implicit `enabled` flag, and nothing else.
        if let Some(effect) = manifest.effects.iter().find(|effect| effect.name == *ty) {
            let display = effect.display_name.as_deref().unwrap_or(&effect.name);
            for param in &effect.params {
                items.push(CompletionItem {
                    label: param.name.clone(),
                    kind: CompletionKind::Property,
                    detail: Some(format!("{display} parameter")),
                    documentation: None,
                    insert_text: Some(format!("{}: ", param.name)),
                });
            }
            items.push(CompletionItem {
                label: "enabled".to_string(),
                kind: CompletionKind::Property,
                detail: Some(format!("{display} parameter")),
                documentation: None,
                insert_text: Some("enabled: ".to_string()),
            });
            return items;
        }
        if let Some(effect) = animatix_syntax::schema::effect_spec(ty) {
            for param in &effect.params {
                items.push(CompletionItem {
                    label: param.name.to_string(),
                    kind: CompletionKind::Property,
                    detail: Some(format!("{} parameter", effect.display_name)),
                    documentation: None,
                    insert_text: Some(format!("{}: ", param.name)),
                });
            }
            items.push(CompletionItem {
                label: "enabled".to_string(),
                kind: CompletionKind::Property,
                detail: Some(format!("{} parameter", effect.display_name)),
                documentation: None,
                insert_text: Some("enabled: ".to_string()),
            });
            return items;
        }

        if let Some(props) = symbols.properties.get(ty) {
            for prop in props {
                let extension = manifest
                    .properties
                    .iter()
                    .find(|property| {
                        property.name == *prop && property.actor_types.iter().any(|a| a == ty)
                    })
                    .map(|property| {
                        (
                            format!("Extension property of {}", ty),
                            format!("Extension type: {:?}", property.ty),
                        )
                    });
                let (detail, doc) =
                    extension.map(|(detail, doc)| (detail, Some(doc))).unwrap_or_else(|| {
                        (
                            format!("Property of {}", ty),
                            property_documentation(prop).map(str::to_string),
                        )
                    });
                items.push(CompletionItem {
                    label: prop.clone(),
                    kind: CompletionKind::Property,
                    detail: Some(detail),
                    documentation: doc,
                    insert_text: Some(format!("{}: ", prop)),
                });
            }
        }
    }

    // Also suggest common properties
    for prop in &[
        "position", "anchor", "offset", "scale", "rotation", "opacity", "color",
    ] {
        if !items.iter().any(|i| &i.label == prop) {
            let doc = property_documentation(prop);
            items.push(CompletionItem {
                label: prop.to_string(),
                kind: CompletionKind::Property,
                detail: Some("Common property".to_string()),
                documentation: doc.map(|s| s.to_string()),
                insert_text: Some(format!("{}: ", prop)),
            });
        }
    }

    items
}

/// Documentation for common properties.
pub fn property_documentation(name: &str) -> Option<&str> {
    match name {
        "position" | "at" => Some("Position as (x, y) coordinates"),
        "anchor" => Some("Anchor point for positioning"),
        "offset" => Some("Offset from position"),
        "scale" => Some("Scale factor (1.0 = normal)"),
        "rotation" => Some("Rotation in degrees"),
        "opacity" => Some("Opacity (0.0 = transparent, 1.0 = opaque)"),
        "color" => Some("Color value (name, hex, or rgb)"),
        "content" => Some("Text content"),
        "font_size" => Some("Font size in pixels"),
        "font_family" => Some("Font family name"),
        "font_weight" => Some(
            "Font weight: 100-900, or named (normal=400, bold=700, light=300, medium=500, semibold=600, black=900)",
        ),
        "font_style" => Some("Font style: 'normal' or 'italic'"),
        "line_height" => Some("Line height multiplier (default 1.2)"),
        "letter_spacing" => Some("Letter spacing in points (default 0)"),
        "word_spacing" => Some("Word spacing in points (default 0)"),
        "max_width" => Some("Max width for text wrapping (0 = no wrap)"),
        "text_align" => Some("Text alignment (left, center, right, justify)"),
        "text_anchor" => Some(
            "Text anchor placement relative to 'at': 'left', 'center', 'right' (default 'center')",
        ),
        "overflow" => Some("Overflow behavior (visible, clip, ellipsis)"),
        "fill" => Some("Fill color"),
        "stroke" => Some("Stroke color"),
        "stroke_width" => Some("Stroke width in pixels"),
        "size" => Some("Size as (width, height)"),
        "radius" => Some("Radius for circles/rounded shapes"),
        "start" => Some("Start point as (x, y)"),
        "end" => Some("End point as (x, y)"),
        "url" => Some("URL for image/svg"),
        "text" => Some("Button text"),
        "x_range" => Some("X axis range"),
        "y_range" => Some("Y axis range"),
        "function" => Some("Function to plot"),
        _ => None,
    }
}
