//! Prebaked rolling counter / odometer primitive.
//!
//! Extracts digit glyphs `0..=9`, `","`, `"."`, `"-"` at build time once,
//! unifies column width to `col_width = max(advance(0..=9))` so proportional
//! fonts do not jitter horizontally, and renders vertical rolling digits
//! with baseline/slot clipping.

use std::sync::Arc;
use kurbo::{Affine, Shape as _};

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{AssignmentCtx, BuildCtx, EvaluateCtx, Primitive, RenderCommand};
use crate::renderer::error::RenderError;
use crate::renderer::text::TextKind;
use crate::renderer::types::{GradientSpec, TextPath};
use crate::timeline::{
    ActorField, AnimationTrack, Environment, SceneDimensions, TrackAccessor,
};

/// Prebaked data for rolling counter glyphs.
#[derive(Clone, Debug)]
pub struct CounterData {
    /// Digit paths for '0'..='9', centered within `col_width`.
    pub digits: Vec<Vec<TextPath>>,
    /// Monospace column width for digits.
    pub col_width: f32,
    /// Line / slot height for clipping and vertical travel.
    pub slot_height: f32,
    /// Comma paths and width.
    pub comma: (Vec<TextPath>, f32),
    /// Decimal point paths and width.
    pub dot: (Vec<TextPath>, f32),
    /// Minus sign paths and width.
    pub minus: (Vec<TextPath>, f32),
    /// Prefix paths and width.
    pub prefix: (Vec<TextPath>, f32),
    /// Suffix paths and width.
    pub suffix: (Vec<TextPath>, f32),
    /// Initial value.
    pub initial_value: f32,
    /// Decimals count.
    pub decimals: u32,
    /// Comma flag.
    pub use_comma: bool,
    /// Font ascent for baseline alignment and clip bounds.
    pub ascent: f32,
    /// Font descent.
    pub descent: f32,
}

pub struct PrebakedCounter {
    pub data: CounterData,
    pub initial_half_size: [f32; 2],
}

pub fn prebake_counter(
    font_family: &str,
    font_size: f32,
    font_weight: f32,
    font_style: &str,
    line_height: f32,
    color_rgba: [f32; 4],
    initial_value: f32,
    decimals: u32,
    use_comma: bool,
    prefix_str: &str,
    suffix_str: &str,
    font_ctx: &crate::renderer::text::FontContext,
) -> Result<PrebakedCounter, RenderError> {
    let mut raw_digits = Vec::with_capacity(10);
    let mut widths = Vec::with_capacity(10);
    let mut ascent = 0.0f32;
    let mut descent = 0.0f32;

    for d in 0..=9 {
        let text_s = d.to_string();
        let compiled = crate::renderer::text::compile_text_cached(
            TextKind::Text,
            &text_s,
            "",
            None,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            0.0,
            0.0,
            color_rgba,
            0.0,
            "left",
            "visible",
            true,
            font_ctx,
        )?;
        ascent = ascent.max(compiled.ascent);
        descent = descent.max(compiled.descent);
        let w = compiled.half_size[0] * 2.0;
        widths.push(w);
        raw_digits.push((compiled.paths.to_vec(), w));
    }

    let col_width = widths.iter().copied().fold(0.0f32, f32::max);
    let slot_height = if (ascent + descent) > 0.0 {
        ascent + descent
    } else {
        font_size * line_height
    };

    // Center each digit in col_width
    let mut digits = Vec::with_capacity(10);
    for (paths, w) in raw_digits {
        let dx = (col_width - w) / 2.0;
        digits.push(translate_paths(&paths, dx, 0.0));
    }

    let compile_sym = |sym: &str| -> Result<(Vec<TextPath>, f32), RenderError> {
        if sym.is_empty() {
            return Ok((Vec::new(), 0.0));
        }
        let compiled = crate::renderer::text::compile_text_cached(
            TextKind::Text,
            sym,
            "",
            None,
            font_family,
            font_size,
            font_weight,
            font_style,
            line_height,
            0.0,
            0.0,
            color_rgba,
            0.0,
            "left",
            "visible",
            true,
            font_ctx,
        )?;
        Ok((compiled.paths.to_vec(), compiled.half_size[0] * 2.0))
    };

    let comma = compile_sym(",")?;
    let dot = compile_sym(".")?;
    let minus = compile_sym("-")?;
    let prefix = compile_sym(prefix_str)?;
    let suffix = compile_sym(suffix_str)?;

    let data = CounterData {
        digits,
        col_width,
        slot_height,
        comma,
        dot,
        minus,
        prefix,
        suffix,
        initial_value,
        decimals,
        use_comma,
        ascent,
        descent,
    };

    let (_, initial_clip) = render_counter_columns(&data, initial_value, decimals, use_comma);
    let initial_width = initial_clip.width() as f32;
    let initial_height = initial_clip.height() as f32;
    let initial_half_size = [initial_width / 2.0, initial_height / 2.0];

    Ok(PrebakedCounter {
        data,
        initial_half_size,
    })
}

fn translate_paths(source: &[TextPath], dx: f32, dy: f32) -> Vec<TextPath> {
    let transform = Affine::translate((dx as f64, dy as f64));
    source
        .iter()
        .map(|tp| {
            let mut new_tp = tp.clone();
            new_tp.path = transform * &tp.path;
            new_tp.glyph_center[0] += dx;
            new_tp.glyph_center[1] += dy;
            new_tp
        })
        .collect()
}

pub fn render_counter_columns(
    data: &CounterData,
    value: f32,
    decimals: u32,
    use_comma: bool,
) -> (Vec<TextPath>, kurbo::Rect) {
    let is_negative = value < 0.0;
    let abs_val = value.abs() as f64;
    let scale = 10.0f64.powi(decimals as i32);
    let scaled = abs_val * scale;
    let floor_scaled = scaled.floor() as u64;
    let frac = (scaled - floor_scaled as f64) as f32;

    let int_part = if decimals > 0 {
        floor_scaled / (scale as u64)
    } else {
        floor_scaled
    };
    let int_str = int_part.to_string();

    let frac_str = if decimals > 0 {
        let frac_part = floor_scaled % (scale as u64);
        format!("{:0width$}", frac_part, width = decimals as usize)
    } else {
        String::new()
    };

    // Collect all digits from left to right
    let mut all_digits: Vec<u8> = Vec::new();
    for b in int_str.bytes() {
        all_digits.push(b - b'0');
    }
    for b in frac_str.bytes() {
        all_digits.push(b - b'0');
    }

    let total_digits = all_digits.len();
    let mut roll_f = vec![0.0f32; total_digits];
    if total_digits > 0 {
        roll_f[total_digits - 1] = frac;
        for i in (0..total_digits - 1).rev() {
            if all_digits[i + 1] == 9 && roll_f[i + 1] > 0.0 {
                roll_f[i] = roll_f[i + 1];
            } else {
                roll_f[i] = 0.0;
            }
        }
    }

    enum ColItem<'a> {
        Symbol(&'a [TextPath], f32),
        Digit(u8, f32),
    }

    let mut items: Vec<ColItem> = Vec::new();
    if is_negative && data.minus.1 > 0.0 {
        items.push(ColItem::Symbol(&data.minus.0, data.minus.1));
    }
    if data.prefix.1 > 0.0 {
        items.push(ColItem::Symbol(&data.prefix.0, data.prefix.1));
    }

    let int_digit_count = int_str.len();
    for (idx, ch) in int_str.chars().enumerate() {
        let d = (ch as u8) - b'0';
        let roll = roll_f[idx];
        items.push(ColItem::Digit(d, roll));

        let remaining = int_digit_count - 1 - idx;
        if use_comma && remaining > 0 && remaining % 3 == 0 && data.comma.1 > 0.0 {
            items.push(ColItem::Symbol(&data.comma.0, data.comma.1));
        }
    }

    if decimals > 0 && data.dot.1 > 0.0 {
        items.push(ColItem::Symbol(&data.dot.0, data.dot.1));
        for (idx, ch) in frac_str.chars().enumerate() {
            let d = (ch as u8) - b'0';
            let roll = roll_f[int_digit_count + idx];
            items.push(ColItem::Digit(d, roll));
        }
    }

    if data.suffix.1 > 0.0 {
        items.push(ColItem::Symbol(&data.suffix.0, data.suffix.1));
    }

    let total_width: f32 = items
        .iter()
        .map(|item| match item {
            ColItem::Symbol(_, w) => *w,
            ColItem::Digit(_, _) => data.col_width,
        })
        .sum();

    let mut x = -total_width / 2.0;
    let mut out_paths = Vec::new();
    let slot_h = data.slot_height;

    for item in items {
        match item {
            ColItem::Symbol(sym_paths, w) => {
                out_paths.extend(translate_paths(sym_paths, x, 0.0));
                x += w;
            },
            ColItem::Digit(d, roll) => {
                let digit_idx = (d as usize).min(9);
                if digit_idx < data.digits.len() {
                    let d_paths = &data.digits[digit_idx];
                    out_paths.extend(translate_paths(d_paths, x, -roll * slot_h));
                }
                if roll > 0.0 {
                    let next_d = ((d + 1) % 10) as usize;
                    if next_d < data.digits.len() {
                        let next_paths = &data.digits[next_d];
                        out_paths.extend(translate_paths(next_paths, x, (1.0 - roll) * slot_h));
                    }
                }
                x += data.col_width;
            },
        }
    }

    let clip_rect = kurbo::Rect::new(
        -total_width as f64 / 2.0 - 4.0,
        -data.ascent as f64 - 2.0,
        total_width as f64 / 2.0 + 4.0,
        data.descent as f64 + 2.0,
    );

    (out_paths, clip_rect)
}

/// The `Counter` primitive.
pub struct CounterPrimitive;

/// Singleton instance of `CounterPrimitive`.
pub const COUNTER: CounterPrimitive = CounterPrimitive;

impl Primitive for CounterPrimitive {
    fn type_name(&self) -> &str {
        "Counter"
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        if let Err(e) = ctx.timeline.process_text_actor_decl(
            self.type_name(),
            label,
            props,
            modifiers,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        ) {
            ctx.diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!("{e}"),
                )
                .with_subject(label),
            );
        }
        Ok(())
    }

    fn handle_assignment(
        &self,
        _track: &mut AnimationTrack,
        _property: &str,
        _value: &Expr,
        _ctx: &mut AssignmentCtx,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
    ) -> bool {
        false
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        let Some(ref data) = ctx.track.text.counter_data else {
            return Ok(None);
        };

        let value = match crate::timeline::dispatch::read_property_value(
            ctx.track,
            ActorField::Tagged("value"),
            ctx.time_ms,
        ) {
            Some(crate::timeline::property_engine::PropertyValue::F32(n)) => n,
            _ => data.initial_value,
        };

        let decimals = match crate::timeline::dispatch::read_property_value(
            ctx.track,
            ActorField::Tagged("decimals"),
            ctx.time_ms,
        ) {
            Some(crate::timeline::property_engine::PropertyValue::U32(n)) => n,
            _ => data.decimals,
        };

        let use_comma = match crate::timeline::dispatch::read_property_value(
            ctx.track,
            ActorField::Tagged("comma"),
            ctx.time_ms,
        ) {
            Some(crate::timeline::property_engine::PropertyValue::Bool(b)) => b,
            _ => data.use_comma,
        };

        let (mut paths, clip_rect) = render_counter_columns(data, value, decimals, use_comma);

        // Apply animated color if present on the track
        if let Some(color_track) = &ctx.track.style.color {
            let col = color_track.evaluate(ctx.time_ms);
            let rgba = [
                (col[0] * 255.0) as u8,
                (col[1] * 255.0) as u8,
                (col[2] * 255.0) as u8,
                (col[3] * 255.0) as u8,
            ];
            for p in &mut paths {
                p.color = rgba;
            }
        }

        let fill_gradient = if ctx.track.style.fill_gradient.is_some() {
            Some(Box::new(ctx.track.style.fill_gradient.get(
                ctx.time_ms,
                GradientSpec::default(),
            )))
        } else {
            None
        };

        let clip_path = Some(clip_rect.to_path(0.1));

        Ok(Some(vec![RenderCommand::Text {
            paths: Arc::from(paths),
            fill_gradient,
            clip_path,
        }]))
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new(
                "at",
                Expr::Tuple(vec![
                    Expr::Num(scene.width as f64 / 2.0),
                    Expr::Num(scene.height as f64 / 2.0),
                ]),
            ),
            Property::new("value", Expr::Num(0.0)),
            Property::new("font_size", Expr::Num(48.0)),
        ]
    }
}
