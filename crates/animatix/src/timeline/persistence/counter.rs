//! Rebuild disposable Counter glyph data from the carried authored tracks.

use crate::primitives::counter::prebake_counter;
use crate::timeline::Timeline;
use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_registry::ActorField;

impl Timeline {
    /// Rebuild the Counter's skipped glyph cache after restoring a carry entry.
    ///
    /// Every input comes from the injected entity's collapsed tracks. The source
    /// timeline is deliberately not consulted: serialized carries omit glyph
    /// paths, and source caches may be stale or unavailable.
    pub(super) fn rebuild_carried_counter(&mut self, label: &str) {
        let Some(id) = self.world.get_id(label) else {
            return;
        };
        if self.world.caps(id).text != Some(crate::timeline::actor_caps::TextKind::Counter) {
            return;
        }

        let text = self.world.text_or_default(id);
        let font_family = text
            .font_family
            .as_ref()
            .map(|track| track.evaluate(0))
            .filter(|family| !family.is_empty())
            .unwrap_or_else(|| crate::renderer::text::DEFAULT_FONT_FAMILY.to_owned());
        let font_size = text
            .font_size
            .as_ref()
            .map(|track| track.evaluate(0))
            .unwrap_or(crate::primitives::counter::DEFAULT_FONT_SIZE);
        let font_weight = text.font_weight.as_ref().map(|track| track.evaluate(0)).unwrap_or(400.0);
        let font_style = text
            .font_style
            .as_ref()
            .map(|track| track.evaluate(0))
            .unwrap_or_else(|| "normal".to_owned());
        let line_height = text.line_height.as_ref().map(|track| track.evaluate(0)).unwrap_or(1.2);
        let color = self
            .world
            .style(id)
            .color
            .as_ref()
            .map(|track| track.evaluate(0))
            .unwrap_or([1.0; 4]);
        let property = |name| self.world.read_property_value(id, ActorField::Tagged(name), 0);
        let initial_value = match property("value") {
            Some(PropertyValue::F32(value)) => value,
            Some(other) => {
                tracing::warn!(actor = label, ?other, "carried Counter value has wrong type");
                0.0
            },
            None => 0.0,
        };
        let prefix = match property("prefix") {
            Some(PropertyValue::String(value)) => value,
            Some(other) => {
                tracing::warn!(actor = label, ?other, "carried Counter prefix has wrong type");
                String::new()
            },
            None => String::new(),
        };
        let suffix = match property("suffix") {
            Some(PropertyValue::String(value)) => value,
            Some(other) => {
                tracing::warn!(actor = label, ?other, "carried Counter suffix has wrong type");
                String::new()
            },
            None => String::new(),
        };
        let decimals = match property("decimals") {
            Some(PropertyValue::U32(value)) => value,
            Some(other) => {
                tracing::warn!(actor = label, ?other, "carried Counter decimals has wrong type");
                0
            },
            None => 0,
        };
        let comma = match property("comma") {
            Some(PropertyValue::Bool(value)) => value,
            Some(other) => {
                tracing::warn!(actor = label, ?other, "carried Counter comma has wrong type");
                false
            },
            None => false,
        };

        match prebake_counter(
            &font_family,
            font_size,
            font_weight,
            &font_style,
            line_height,
            color,
            initial_value,
            decimals,
            comma,
            &prefix,
            &suffix,
            self.font_context.as_ref(),
        ) {
            Ok(prebaked) => {
                if let Some(text) = self.world_mut().text.get_mut(id) {
                    text.counter_data = Some(prebaked.data);
                } else {
                    tracing::warn!(actor = label, "carried Counter has no text component");
                }
            },
            Err(error) => tracing::warn!(
                actor = label,
                %error,
                "failed to rebuild carried Counter glyph cache"
            ),
        }
    }
}
