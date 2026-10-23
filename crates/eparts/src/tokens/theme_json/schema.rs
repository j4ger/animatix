/// JSON Schema for the theme file format.
///
/// The schema is intentionally permissive at the root (string values for every
/// color field) and does not enumerate every nested token individually; editors
/// can use it for basic validation and auto-completion while the Rust DTO
/// remains the authoritative parser.
///
/// **Keep in sync with the `partial_group!` invocations.** This schema is
/// hand-written (not generated from the macro groups); when a group's field
/// list or a `PartialTheme` field name changes, update the matching
/// `$defs`/property entry below. The nested `colorGroup`/`slotSlots`
/// `patternProperties` cover additive color/slot fields automatically, but a
/// *new group name* or a renamed field must be added here by hand.
pub fn theme_schema_json() -> String {
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "Animatix Theme",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "name": { "type": "string" },
            "extends": { "type": "string" },
            "dark": { "$ref": "#/$defs/theme" },
            "light": { "$ref": "#/$defs/theme" }
        },
        "$defs": {
            "theme": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "surface": { "$ref": "#/$defs/colorGroup" },
                    "text": { "$ref": "#/$defs/colorGroup" },
                    "accent": { "$ref": "#/$defs/colorGroup" },
                    "status": { "$ref": "#/$defs/colorGroup" },
                    "border": { "$ref": "#/$defs/colorGroup" },
                    "overlay": { "$ref": "#/$defs/colorGroup" },
                    "lines": { "$ref": "#/$defs/colorGroup" },
                    "button": { "$ref": "#/$defs/buttonSlots" },
                    "list": { "$ref": "#/$defs/fillSlots" },
                    "tab": { "$ref": "#/$defs/tabSlots" },
                    "menu_item": { "$ref": "#/$defs/slotSlots" },
                    "input": { "$ref": "#/$defs/slotSlots" },
                    "scrollbar": { "$ref": "#/$defs/scrollbar" },
                    "toggle": { "$ref": "#/$defs/toggleSlots" },
                    "progress": { "$ref": "#/$defs/colorGroup" },
                    "badge": { "$ref": "#/$defs/slot" },
                    "tag": { "$ref": "#/$defs/tagSlots" },
                    "kbd": { "$ref": "#/$defs/slot" },
                    "tooltip": { "$ref": "#/$defs/slot" },
                    "alert": { "$ref": "#/$defs/alertSlots" },
                    "toast": { "$ref": "#/$defs/toastSlots" },
                    "skeleton": { "$ref": "#/$defs/colorGroup" },
                    "color_picker": { "$ref": "#/$defs/colorGroup" },
                    "easing_curve": { "$ref": "#/$defs/colorGroup" },
                    "elevation": { "$ref": "#/$defs/elevation" }
                }
            },
            "colorGroup": {
                "type": "object",
                "additionalProperties": false,
                "patternProperties": {
                    "^[a-z_]+$": { "type": "string" }
                }
            },
            "toggleSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "checked": { "$ref": "#/$defs/slot" },
                    "unchecked": { "$ref": "#/$defs/slot" },
                    "hover": { "$ref": "#/$defs/slot" },
                    "disabled": { "$ref": "#/$defs/slot" },
                    "thumb": { "type": "string" },
                    "mark": { "type": "string" }
                }
            },
            "tagSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "normal": { "$ref": "#/$defs/slot" },
                    "hover": { "$ref": "#/$defs/slot" }
                }
            },
            "alertLevel": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" },
                    "accent": { "type": "string" },
                    "icon": { "type": "string" }
                }
            },
            "alertSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "info": { "$ref": "#/$defs/alertLevel" },
                    "success": { "$ref": "#/$defs/alertLevel" },
                    "warning": { "$ref": "#/$defs/alertLevel" },
                    "error": { "$ref": "#/$defs/alertLevel" }
                }
            },
            "toastLevel": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" },
                    "border": { "type": "string" },
                    "accent": { "type": "string" }
                }
            },
            "toastSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "info": { "$ref": "#/$defs/toastLevel" },
                    "success": { "$ref": "#/$defs/toastLevel" },
                    "warning": { "$ref": "#/$defs/toastLevel" },
                    "error": { "$ref": "#/$defs/toastLevel" }
                }
            },
            "slot": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" },
                    "border": { "type": "string" }
                }
            },
            "fill": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" }
                }
            },
            "tab": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bg": { "type": "string" },
                    "fg": { "type": "string" },
                    "indicator": { "type": "string" }
                }
            },
            "buttonState": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "normal": { "$ref": "#/$defs/slot" },
                    "hover": { "$ref": "#/$defs/slot" },
                    "active": { "$ref": "#/$defs/slot" },
                    "selected": { "$ref": "#/$defs/slot" },
                    "disabled": { "$ref": "#/$defs/slot" },
                    "focus": { "$ref": "#/$defs/slot" }
                }
            },
            "buttonSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "primary": { "$ref": "#/$defs/buttonState" },
                    "secondary": { "$ref": "#/$defs/buttonState" },
                    "ghost": { "$ref": "#/$defs/buttonState" },
                    "icon": { "$ref": "#/$defs/buttonState" },
                    "danger": { "$ref": "#/$defs/buttonState" }
                }
            },
            "fillSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "even": { "$ref": "#/$defs/fill" },
                    "odd": { "$ref": "#/$defs/fill" },
                    "selected": { "$ref": "#/$defs/fill" },
                    "hover": { "$ref": "#/$defs/fill" }
                }
            },
            "tabSlots": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "active": { "$ref": "#/$defs/tab" },
                    "inactive": { "$ref": "#/$defs/tab" },
                    "hover": { "$ref": "#/$defs/tab" }
                }
            },
            "slotSlots": {
                "type": "object",
                "additionalProperties": false,
                "patternProperties": {
                    "^[a-z_]+$": { "$ref": "#/$defs/slot" }
                }
            },
            "scrollbar": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "thumb": { "type": "string" },
                    "thumb_hover": { "type": "string" }
                }
            },
            "elevation": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "raised": { "$ref": "#/$defs/shadow" },
                    "overlay": { "$ref": "#/$defs/shadow" }
                }
            },
            "shadow": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "offset": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "minItems": 2,
                        "maxItems": 2
                    },
                    "blur": { "type": "integer" },
                    "spread": { "type": "integer" },
                    "color": { "type": "string" }
                }
            }
        }
    });
    serde_json::to_string_pretty(&schema).expect("theme schema is valid JSON")
}
