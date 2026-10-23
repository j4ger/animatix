// ── Serde helpers (theme-json feature) ────────────────────────────────

#[cfg(feature = "theme-json")]
pub(crate) mod serde_color32 {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize colors as hex strings (`#rrggbb` or `#rrggbbaa`).
    pub fn serialize<S: Serializer>(color: &Color32, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&color.to_hex())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Color32, D::Error> {
        let text = String::deserialize(deserializer)?;
        Color32::from_hex(&text)
            .map_err(|e| serde::de::Error::custom(format!("invalid color hex {text:?}: {e:?}")))
    }
}

#[cfg(feature = "theme-json")]
pub(crate) mod serde_shadow {
    use egui::{Color32, Shadow};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use serde_json::Value;

    pub fn serialize<S: Serializer>(shadow: &Shadow, serializer: S) -> Result<S::Ok, S::Error> {
        let value = serde_json::json!({
            "offset": shadow.offset,
            "blur": shadow.blur,
            "spread": shadow.spread,
            "color": shadow.color.to_hex(),
        });
        value.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Shadow, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let offset = value
            .get("offset")
            .and_then(Value::as_array)
            .and_then(|items| Some([as_i8(&items[0])?, as_i8(&items[1])?]))
            .ok_or_else(|| serde::de::Error::custom("shadow offset must be [x, y]"))?;
        let blur = value.get("blur").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok());
        let spread = value.get("spread").and_then(Value::as_u64).and_then(|v| u8::try_from(v).ok());
        let color = value
            .get("color")
            .and_then(Value::as_str)
            .and_then(|text| Color32::from_hex(text).ok())
            .ok_or_else(|| serde::de::Error::custom("shadow color must be a hex string"))?;
        Ok(Shadow {
            offset,
            blur: blur.ok_or_else(|| serde::de::Error::custom("shadow blur must be a u8"))?,
            spread: spread.ok_or_else(|| serde::de::Error::custom("shadow spread must be a u8"))?,
            color,
        })
    }

    fn as_i8(value: &Value) -> Option<i8> {
        value.as_i64().and_then(|v| i8::try_from(v).ok())
    }
}
