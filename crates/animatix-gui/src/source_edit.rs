//! Surgical source text editing for .amx files.
//!
//! When a user edits a property via the inspector widget or drags a bounding box handle,
//! we use the `SourceIndex` to find the byte span of the old value and replace just
//! that portion with the serialized new value, preserving the rest of the source text.

use animatix::ast::ByteSpan;
use crate::app::workspace::PropertyValue;

/// Apply a surgical source edit: replace the text at `span` with `replacement`.
///
/// Returns the modified source text. This is a pure function that doesn't
/// interact with the filesystem.
pub(crate) fn apply_source_edit(source: &str, span: &ByteSpan, replacement: &str) -> String {
    let mut result = String::with_capacity(source.len() + replacement.len());
    result.push_str(&source[..span.start]);
    result.push_str(replacement);
    result.push_str(&source[span.end..]);
    result
}

/// Serialize a `PropertyValue` to its source text representation.
///
/// This is the inverse of what the parser produces, used for writing edits back.
pub(crate) fn serialize_property_value(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Vec2([x, y]) => {
            // Check if values are integers or floats
            if x.fract() == 0.0 && y.fract() == 0.0 {
                format!("({}, {})", *x as i32, *y as i32)
            } else {
                format!("({}, {})", x, y)
            }
        }
        PropertyValue::Float(v) => {
            if v.fract() == 0.0 {
                format!("{}", *v as i32)
            } else {
                format!("{}", v)
            }
        }
        PropertyValue::Color([r, g, b, a]) => {
            // Serialize color as rgba(r, g, b, a) with values 0-1
            if (a - 1.0).abs() < 0.001 {
                // Opaque color, use rgb() shorthand if all values are simple
                if r.fract() == 0.0 && g.fract() == 0.0 && b.fract() == 0.0 {
                    format!("rgb({}, {}, {})", (*r * 255.0) as i32, (*g * 255.0) as i32, (*b * 255.0) as i32)
                } else {
                    format!("rgba({}, {}, {}, {})", r, g, b, a)
                }
            } else {
                format!("rgba({}, {}, {}, {})", r, g, b, a)
            }
        }
        PropertyValue::Text(s) => {
            // Escape quotes in string
            let escaped = s.replace('"', "\\\"");
            format!("\"{}\"", escaped)
        }
    }
}

/// Serialize a property value, handling size half-size scaling.
///
/// Timeline stores size as full-size, but the source may use half-size (for radius).
/// When the property is "size" and we detect it might be a half-size value,
/// we need to scale appropriately.
///
/// For now, we handle this by accepting a flag. If `is_half_size` is true,
/// we double the values before serializing.
pub(crate) fn serialize_size_value(value: &PropertyValue, is_half_size: bool) -> String {
    match value {
        PropertyValue::Vec2([w, h]) => {
            let w = if is_half_size { w * 2.0 } else { *w };
            let h = if is_half_size { h * 2.0 } else { *h };
            if w.fract() == 0.0 && h.fract() == 0.0 {
                format!("({}, {})", w as i32, h as i32)
            } else {
                format!("({}, {})", w, h)
            }
        }
        _ => serialize_property_value(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_source_edit_middle_replacement() {
        let source = "Hello world! This is a test.";
        let span = ByteSpan { start: 6, end: 11 }; // "world"
        let result = apply_source_edit(source, &span, "Rust");
        assert_eq!(result, "Hello Rust! This is a test.");
    }

    #[test]
    fn apply_source_edit_start_replacement() {
        let source = "Hello world!";
        let span = ByteSpan { start: 0, end: 5 }; // "Hello"
        let result = apply_source_edit(source, &span, "Goodbye");
        assert_eq!(result, "Goodbye world!");
    }

    #[test]
    fn apply_source_edit_end_replacement() {
        let source = "Hello world!";
        let span = ByteSpan { start: 6, end: 12 }; // "world!"
        let result = apply_source_edit(source, &span, "Universe!");
        assert_eq!(result, "Hello Universe!");
    }

    #[test]
    fn apply_source_edit_preserves_exact_spans() {
        // Original: "width: 100, height: 200"
        let source = "width: 100, height: 200";
        // Replace just "100" with "150"
        let span = ByteSpan { start: 7, end: 10 };
        let result = apply_source_edit(source, &span, "150");
        assert_eq!(result, "width: 150, height: 200");
    }

    #[test]
    fn serialize_vec2_integer() {
        let value = PropertyValue::Vec2([100.0, 200.0]);
        assert_eq!(serialize_property_value(&value), "(100, 200)");
    }

    #[test]
    fn serialize_vec2_float() {
        let value = PropertyValue::Vec2([100.5, 200.7]);
        assert_eq!(serialize_property_value(&value), "(100.5, 200.7)");
    }

    #[test]
    fn serialize_float_integer() {
        let value = PropertyValue::Float(42.0);
        assert_eq!(serialize_property_value(&value), "42");
    }

    #[test]
    fn serialize_float_float() {
        let value = PropertyValue::Float(42.5);
        assert_eq!(serialize_property_value(&value), "42.5");
    }

    #[test]
    fn serialize_color_opaque() {
        let value = PropertyValue::Color([1.0, 0.0, 0.0, 1.0]); // Red
        let result = serialize_property_value(&value);
        // Since all values are 0 or 1, it uses the integer form
        assert!(result.contains("rgb"));
    }

    #[test]
    fn serialize_color_with_alpha() {
        let value = PropertyValue::Color([0.5, 0.5, 0.5, 0.8]);
        let result = serialize_property_value(&value);
        assert!(result.contains("rgba"));
        assert!(result.contains("0.5"));
    }

    #[test]
    fn serialize_text() {
        let value = PropertyValue::Text("Hello, World!".to_string());
        assert_eq!(serialize_property_value(&value), "\"Hello, World!\"");
    }

    #[test]
    fn serialize_text_escapes_quotes() {
        let value = PropertyValue::Text("Say \"hello\"".to_string());
        assert_eq!(serialize_property_value(&value), "\"Say \\\"hello\\\"\"");
    }

    #[test]
    fn serialize_size_value_doubles_when_half_size() {
        let value = PropertyValue::Vec2([50.0, 50.0]);
        // Simulating radius (half-size) being doubled for width/height
        let result = serialize_size_value(&value, true);
        assert_eq!(result, "(100, 100)");
    }

    #[test]
    fn serialize_size_value_no_double_when_not_half_size() {
        let value = PropertyValue::Vec2([100.0, 100.0]);
        let result = serialize_size_value(&value, false);
        assert_eq!(result, "(100, 100)");
    }

    #[test]
    fn roundtrip_size_edit_preserves_values() {
        // Simulate the exact bug: dragging a size handle
        // Original source: "backdrop: Rect, size: (2494.552, 1377.7778)"
        // After drag, the new size is (2509.0366, 671.8605) - these are FULL size values
        // The span covers "(2494.552, 1377.7778)"
        let source = "backdrop: Rect, size: (2494.552, 1377.7778), color: scene.background";
        let span = ByteSpan {
            start: source.find("(2494.552").unwrap(),
            end: source.find("1377.7778)").unwrap() + "1377.7778)".len(),
        };

        // The drag handler sends full-size values
        let new_value = PropertyValue::Vec2([2509.0366, 671.8605]);

        // BUG: serialize_size_value with is_half_size=true doubles the values!
        let serialized_wrong = serialize_size_value(&new_value, true);
        let result_wrong = apply_source_edit(source, &span, &serialized_wrong);
        // This would produce: "size: (5018.0732, 1343.721)" - wrong!

        // CORRECT: should NOT double since drag already sends full-size
        let serialized_correct = serialize_size_value(&new_value, false);
        let result_correct = apply_source_edit(source, &span, &serialized_correct);
        assert_eq!(
            result_correct,
            "backdrop: Rect, size: (2509.0366, 671.8605), color: scene.background"
        );
    }

    #[test]
    fn roundtrip_position_edit() {
        // Simulate finding position span in: "btn: Button, at: (100, 200)"
        let source = "btn: Button, at: (100, 200)";
        // (100, 200) starts at byte 17 and ends at byte 27
        let span = ByteSpan { start: 17, end: 27 };

        let new_value = PropertyValue::Vec2([150.0, 250.0]);
        let serialized = serialize_property_value(&new_value);

        let result = apply_source_edit(source, &span, &serialized);
        assert_eq!(result, "btn: Button, at: (150, 250)");
    }

    #[test]
    fn roundtrip_color_edit() {
        let source = "btn.color = red";
        // "red" starts at byte 12 and ends at byte 15
        let span = ByteSpan { start: 12, end: 15 };

        let new_value = PropertyValue::Color([0.0, 1.0, 0.0, 1.0]);
        let serialized = serialize_property_value(&new_value);

        let result = apply_source_edit(source, &span, &serialized);
        // The serialized color should be rgb(0, 255, 0) for green
        assert!(result.contains("rgb"));
        assert!(result.contains("255"));
    }
}
