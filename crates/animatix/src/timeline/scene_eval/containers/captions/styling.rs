use super::{ActiveCue, PropertyPlan, map_timed_words_to_char_ranges};
use crate::primitives::surface::paint_color;

/// Select a phrase delimited by real word-timestamp gaps. Keep authored text
/// (including CJK spacing and line breaks) through the existing source map.
pub(super) fn cue_phrase(cue: &ActiveCue, pause: f64, time_s: f64) -> Option<ActiveCue> {
    if pause <= 0.0 || cue.words.len() < 2 {
        return None;
    }
    let mut first = 0;
    let mut last = cue.words.len();
    for (index, pair) in cue.words.windows(2).enumerate() {
        if pair[1].start_s - pair[0].end_s >= pause {
            if cue.speech_start_s + pair[1].start_s > time_s {
                last = index + 1;
                break;
            }
            first = index + 1;
        }
    }
    if first == 0 && last == cue.words.len() {
        return None;
    }
    let start_char = if first == 0 {
        0
    } else {
        cue.word_char_ranges.get(first)?.as_ref()?.start
    };
    let end_char = if last == cue.words.len() {
        cue.text.chars().count()
    } else {
        cue.word_char_ranges.get(last)?.as_ref()?.start
    };
    let text: String = cue
        .text
        .chars()
        .skip(start_char)
        .take(end_char.checked_sub(start_char)?)
        .collect();
    let text = text.trim().to_owned();
    let words = cue.words[first..last].to_vec();
    Some(ActiveCue {
        start_s: if first == 0 {
            cue.start_s
        } else {
            cue.speech_start_s + words[0].start_s
        },
        end_s: cue
            .words
            .get(last)
            .map_or(cue.end_s, |word| (cue.speech_start_s + word.start_s).min(cue.end_s)),
        word_char_ranges: map_timed_words_to_char_ranges(&text, &words, "caption phrase"),
        speech_start_s: cue.speech_start_s,
        real_word_timings: cue.real_word_timings,
        active_color: cue.active_color,
        text,
        words,
    })
}

/// Fit independent cue and page fades inside the currently visible interval.
pub(super) fn transition_alpha(
    cue: &ActiveCue,
    time_s: f64,
    time_ms: u64,
    (start, end): (f64, f64),
    property_plan: &PropertyPlan,
    paint_overrides: Option<&std::collections::HashMap<String, crate::timeline::Value>>,
) -> f32 {
    let number = |name: &str| match paint_overrides.and_then(|values| values.get(name)) {
        Some(crate::timeline::Value::Num(value)) => value.max(0.0),
        _ => f64::from(property_plan.sample_f32_prop(name, time_ms, 0.0).max(0.0)),
    };
    let entrance = number("caption_enter");
    let legacy = if entrance == 0.0 {
        number("transition_time")
    } else {
        0.0
    };
    let page_fade = number("caption_page_transition");
    let enter = if start == cue.start_s {
        entrance
    } else {
        page_fade
    };
    let exit = if end == cue.end_s {
        number("caption_exit")
    } else {
        page_fade
    };
    let fit = if enter + exit > 0.0 {
        ((end - start) / (enter + exit)).min(1.0)
    } else {
        1.0
    };
    let fade = crate::primitives::captions::cue_transition_alpha;
    fade(time_s, cue.start_s, legacy)
        * fade(time_s, start, enter * fit)
        * fade(end - time_s, 0.0, exit * fit)
}

pub(super) struct CaptionPaint {
    outline: [f32; 4],
    outline_width: f32,
    shadow: [f32; 2],
    shadow_color: [f32; 4],
    pub(super) word_backdrop: [f32; 4],
}

impl CaptionPaint {
    pub(super) fn sample(
        plan: &PropertyPlan,
        time: u64,
        overrides: Option<&std::collections::HashMap<String, crate::timeline::Value>>,
    ) -> Self {
        use crate::timeline::Value;
        let get_color = |name, default| {
            overrides
                .and_then(|values| crate::primitives::styling::override_color(values, name))
                .unwrap_or_else(|| plan.sample_color_prop(name, time, default))
        };
        Self {
            outline: get_color("caption_outline", [0.0, 0.0, 0.0, 1.0]),
            outline_width: match overrides.and_then(|values| values.get("caption_outline_width")) {
                Some(Value::Num(value)) => *value as f32,
                _ => plan.sample_f32_prop("caption_outline_width", time, 0.0),
            },
            shadow: match overrides.and_then(|values| values.get("caption_shadow")) {
                Some(Value::Vec2(value)) => value.map(|n| n as f32),
                _ => plan.sample_vec2_prop("caption_shadow", time, [0.0; 2]),
            },
            shadow_color: get_color("caption_shadow_color", [0.0, 0.0, 0.0, 0.75]),
            word_backdrop: get_color("word_backdrop_color", [0.0; 4]),
        }
    }

    pub(super) fn draw(
        &self,
        scene: &mut crate::scene::Scene,
        glyph: &crate::renderer::types::TextPath,
        transform: kurbo::Affine,
        color: peniko::Color,
        opacity: f32,
    ) {
        let opacity = opacity * glyph.opacity;
        if self.shadow != [0.0; 2] {
            let [x, y] = self.shadow.map(f64::from);
            scene.fill(
                peniko::Fill::NonZero,
                transform * kurbo::Affine::translate((x, y)),
                paint_color(self.shadow_color, opacity),
                None,
                &glyph.path,
            );
        }
        if self.outline_width > 0.0 {
            let stroke = kurbo::Stroke::new(f64::from(self.outline_width) * 2.0)
                .with_join(kurbo::Join::Round);
            scene.stroke(&stroke, transform, paint_color(self.outline, opacity), None, &glyph.path);
        }
        scene.fill(peniko::Fill::NonZero, transform, color, None, &glyph.path);
    }
}
