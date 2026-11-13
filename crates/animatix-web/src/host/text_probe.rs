//! Explicit-font text snapshots shared by native and wasm parity runners.

use animatix::renderer::text::{
    FontContext, TextCompileRequest, TextKind, TextStyle, compile_text_cached,
    compile_typst_grouped_cached,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextProbeRequest {
    pub cases: Vec<TextProbeCase>,
    #[serde(default)]
    pub grouped: Vec<GroupedProbeCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextProbeCase {
    pub name: String,
    pub text: String,
    pub family: String,
    pub size: f32,
    pub weight: f32,
    #[serde(default = "normal_style")]
    pub style: String,
    #[serde(default)]
    pub max_width: f32,
    #[serde(default)]
    pub max_lines: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GroupedProbeCase {
    pub name: String,
    pub markup: String,
    pub family: String,
    pub size: f32,
    pub weight: f32,
    #[serde(default)]
    pub expected_groups: Option<usize>,
}

fn normal_style() -> String {
    "normal".to_owned()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextProbeOutput {
    pub font_sha256: Vec<String>,
    pub cases: Vec<TextCaseOutput>,
    pub grouped: Vec<GroupedCaseOutput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TextCaseOutput {
    pub name: String,
    pub ascent: f32,
    pub descent: f32,
    pub baseline_offset: f32,
    pub half_size: [f32; 2],
    pub glyphs: Vec<GlyphOutput>,
    pub hardware_runs: Vec<HardwareRunOutput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GlyphOutput {
    pub char_idx: u16,
    pub word_idx: u16,
    pub line_idx: u16,
    pub center: [f32; 2],
    pub bounds: [f64; 4],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HardwareRunOutput {
    pub font_sha256: String,
    pub font_index: u32,
    pub normalized_coords: Vec<i16>,
    pub font_size: f32,
    pub glyphs: Vec<HardwareGlyphOutput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct HardwareGlyphOutput {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GroupedCaseOutput {
    pub name: String,
    pub ranges: Vec<[usize; 2]>,
    pub glyphs: Vec<GlyphOutput>,
}

/// Compile requests with exactly the supplied faces and expose deterministic,
/// serialization-friendly shaping and outline facts.
pub fn probe_text(
    request: &TextProbeRequest,
    fonts: &[Vec<u8>],
) -> Result<TextProbeOutput, String> {
    if fonts.is_empty() {
        return Err("text parity requires at least one explicit font".into());
    }
    let mut context = FontContext::empty();
    context.load_fonts(fonts.iter().cloned());
    let font_sha256 = fonts.iter().map(|bytes| digest(bytes)).collect::<Vec<_>>();

    let mut cases = Vec::with_capacity(request.cases.len());
    for case in &request.cases {
        let style = TextStyle {
            font_family: &case.family,
            font_size: case.size,
            font_weight: case.weight,
            font_style: &case.style,
            ..TextStyle::new(&case.family, case.size)
        };
        let mut compile = TextCompileRequest::new(TextKind::Text, &case.text, style);
        compile.max_width = case.max_width;
        compile.max_lines = case.max_lines;
        compile.allow_fast_path = true;
        let compiled =
            compile_text_cached(&compile, &context).map_err(|error| error.to_string())?;
        let glyphs = compiled.paths.iter().map(glyph_output).collect();
        let mut hardware_runs = Vec::new();
        let Some(layout) = &compiled.hardware_layout else {
            return Err(format!("{} did not produce a hardware glyph layout", case.name));
        };
        for run in &layout.runs {
            let run_digest = digest(&run.font_data);
            if !font_sha256.iter().any(|allowed| allowed == &run_digest) {
                return Err(format!("{} selected an unprovided font", case.name));
            }
            if run.glyphs.iter().any(|glyph| glyph.id == 0) {
                return Err(format!("{} contains a missing-font glyph", case.name));
            }
            hardware_runs.push(HardwareRunOutput {
                font_sha256: run_digest,
                font_index: run.font_index,
                normalized_coords: run.normalized_coords.to_vec(),
                font_size: run.font_size,
                glyphs: run
                    .glyphs
                    .iter()
                    .map(|glyph| HardwareGlyphOutput {
                        id: glyph.id,
                        x: glyph.x,
                        y: glyph.y,
                    })
                    .collect(),
            });
        }
        cases.push(TextCaseOutput {
            name: case.name.clone(),
            ascent: compiled.ascent,
            descent: compiled.descent,
            baseline_offset: compiled.baseline_offset,
            half_size: compiled.half_size,
            glyphs,
            hardware_runs,
        });
    }

    let mut grouped = Vec::with_capacity(request.grouped.len());
    for case in &request.grouped {
        let compiled = compile_typst_grouped_cached(
            &case.markup,
            case.size,
            [1.0; 4],
            &case.family,
            &context,
            case.weight,
            "normal",
            1.2,
            0.0,
            0.0,
        )
        .map_err(|error| error.to_string())?;
        if let Some(expected) = case.expected_groups
            && compiled.ranges.len() != expected
        {
            return Err(format!(
                "{} produced {} fragment ranges; expected {expected}",
                case.name,
                compiled.ranges.len()
            ));
        }
        grouped.push(GroupedCaseOutput {
            name: case.name.clone(),
            ranges: compiled.ranges.iter().map(|range| [range.start, range.end]).collect(),
            glyphs: compiled.glyphs.iter().map(glyph_output).collect(),
        });
    }

    Ok(TextProbeOutput {
        font_sha256,
        cases,
        grouped,
    })
}

fn glyph_output(glyph: &animatix::renderer::text::TextPath) -> GlyphOutput {
    GlyphOutput {
        char_idx: glyph.char_idx,
        word_idx: glyph.word_idx,
        line_idx: glyph.line_idx,
        center: glyph.glyph_center,
        bounds: [
            glyph.bounds.x0,
            glyph.bounds.y0,
            glyph.bounds.x1,
            glyph.bounds.y1,
        ],
    }
}

fn digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}
