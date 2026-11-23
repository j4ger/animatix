use animatix::timeline::Timeline;
use animatix_render::offscreen::RenderedFrame;

pub const SIZES: [u32; 3] = [64, 128, 256];
pub const WIDTH: u32 = 512;
pub const RADIUS: u32 = 6;
pub const WARMUP: usize = 4;

pub fn source(size: u32, blur: bool) -> String {
    let rect = format!(
        "box: Rect, size: ({size}, {size}), color: (1, 1, 1, 1), opacity: 1, stroke_width: 0"
    );
    let actor = if blur {
        format!(
            "panel: Filter, at: (256, 256), opacity: 1 {{\nsoft: Blur, radius: {RADIUS}\n{rect}\n}}"
        )
    } else {
        format!("{rect}, at: (256, 256)")
    };
    format!(
        "config {{ canvas_size: (512, 512) }}\n#0s\nbackground: Rect, at: (256, 256), size: (512, 512), color: (0, 0, 0, 1), opacity: 1, stroke_width: 0\n{actor}\n"
    )
}

pub fn build(source: &str) -> Result<Timeline, String> {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    if !errors.is_empty() {
        return Err(format!("fixture parse diagnostics: {errors:?}"));
    }
    let ast = ast.ok_or("missing fixture AST")?;
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    if !report.diagnostics.is_empty() {
        return Err(format!("fixture build diagnostics: {:?}", report.diagnostics));
    }
    Ok(report.output)
}

pub fn guard(frame: &RenderedFrame, size: u32, blur: bool) -> Result<(), String> {
    if !SIZES.contains(&size)
        || frame.width != WIDTH
        || frame.height != WIDTH
        || frame.rgba.len() != (WIDTH * WIDTH * 4) as usize
    {
        return Err("invalid dimensions or fixture size".into());
    }
    let at = |x: u32, y: u32| -> u8 { frame.rgba[((y * WIDTH + x) * 4) as usize] };
    if frame
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .any(|p| p[3] < 250 || p[0].abs_diff(p[1]) > 2 || p[0].abs_diff(p[2]) > 2)
    {
        return Err("expected opaque neutral grayscale output".into());
    }
    if at(256, 256) < 220 || at(8, 8) > 5 {
        return Err("missing center or unexpected background".into());
    }
    let edge = 256 + size / 2;
    // Four axial cross-sections prevent a displaced/wrong-sized rectangle
    // from passing a center-only probe. These are broad correctness guards,
    // not cross-device golden images or performance acceptance thresholds.
    for (x, y) in [
        (edge - 12, 256),
        (512 - edge + 11, 256),
        (256, edge - 12),
        (256, 512 - edge + 11),
    ] {
        if at(x, y) < 160 {
            return Err("incorrect interior extent".into());
        }
    }
    for (x, y) in [
        (edge + 24, 256),
        (511 - edge - 24, 256),
        (256, edge + 24),
        (256, 511 - edge - 24),
    ] {
        if at(x, y) > 8 {
            return Err("incorrect exterior extent".into());
        }
    }
    for (inside, outside) in [
        ((edge - 2, 256), (edge + 2, 256)),
        ((256, edge - 2), (256, edge + 2)),
    ] {
        let a = at(inside.0, inside.1);
        let b = at(outside.0, outside.1);
        if blur {
            if !(a > b && a < 250 && b > 3 && b < 180) {
                return Err(format!("blur edge missing or malformed: {a}/{b}"));
            }
        } else if a < 250 || b > 3 {
            return Err(format!("plain edge malformed: {a}/{b}"));
        }
    }
    Ok(())
}

pub fn device_class(kind: wgpu::DeviceType) -> &'static str {
    match kind {
        wgpu::DeviceType::Cpu => "software",
        wgpu::DeviceType::IntegratedGpu | wgpu::DeviceType::DiscreteGpu => "hardware_reported",
        wgpu::DeviceType::VirtualGpu => "virtual_unqualified",
        _ => "unknown_unqualified",
    }
}
