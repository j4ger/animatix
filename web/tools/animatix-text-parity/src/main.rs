use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::BuildQuality;
use animatix_render::offscreen::OffscreenRenderer;
use animatix_web::host::{TextProbeRequest, build_prepared, prepare_document, probe_text};
use base64::Engine as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let first = args
        .next()
        .ok_or("usage: animatix-text-parity REQUEST.json FONT... | --render SCENE TIME FONT...")?;
    if first == "--render" {
        let scene_path = PathBuf::from(args.next().ok_or("--render requires a scene path")?);
        let time = args
            .next()
            .ok_or("--render requires a frame time in seconds")?
            .to_string_lossy()
            .parse::<f64>()?;
        let font_paths = args.map(PathBuf::from).collect::<Vec<_>>();
        return render_scene(scene_path, time, font_paths);
    }
    let request_path = PathBuf::from(first);
    let font_paths = args.map(PathBuf::from).collect::<Vec<_>>();
    if font_paths.is_empty() {
        return Err("provide at least one explicit font file".into());
    }
    let request: TextProbeRequest = serde_json::from_slice(&fs::read(request_path)?)?;
    let fonts = font_paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
    let output = probe_text(&request, &fonts).map_err(std::io::Error::other)?;
    emit_json(&serde_json::to_value(output)?)?;
    Ok(())
}

fn render_scene(
    scene_path: PathBuf,
    time_s: f64,
    font_paths: Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    if font_paths.is_empty() {
        return Err("--render requires at least one explicit font file".into());
    }
    if !time_s.is_finite() || time_s < 0.0 {
        return Err("--render time must be finite and non-negative".into());
    }

    let source = fs::read_to_string(&scene_path)?;
    let fonts = font_paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
    let mut font_context = FontContext::empty();
    font_context.load_fonts(fonts.iter().cloned());
    // Visual fixtures are self-contained. Omitting source_url keeps the native
    // source-access contract aligned with browser prepare_source without
    // inventing a filesystem URL that the module resolver cannot parse.
    let prepared = prepare_document(&source, &[], &[], None);
    if !prepared.analysis.ok {
        return Err(format!("scene analysis failed: {:?}", prepared.analysis.diagnostics).into());
    }
    if !prepared.analysis.assets.is_empty() {
        return Err(format!(
            "render subcommand needs explicit assets: {:?}",
            prepared.analysis.assets
        )
        .into());
    }
    let built = build_prepared(&prepared, Arc::new(font_context), BuildQuality::Production, None);
    let target = built
        .target
        .ok_or_else(|| format!("scene build failed: {:?}", built.result.diagnostics))?;
    let width = built.result.width;
    let height = built.result.height;
    let timeline = match target {
        BuildTarget::SingleScene(timeline) => timeline,
        BuildTarget::MultiScene(_) => {
            return Err("--render accepts single-scene sources only".into());
        },
    };

    let mut renderer = OffscreenRenderer::new().map_err(std::io::Error::other)?;
    let adapter = renderer.device().adapter_info();
    if std::env::var("WPR_GPU").as_deref() == Ok("1") && adapter.vendor != 0x10de {
        return Err(format!(
            "WPR_GPU=1 requires an NVIDIA native adapter, got vendor {:#06x} ({})",
            adapter.vendor, adapter.name
        )
        .into());
    }
    let frame = renderer
        .render_timeline(&timeline, time_s, animatix::timeline::SceneDimensions { width, height })
        .map_err(std::io::Error::other)?;
    let response = serde_json::json!({
        "scene": scene_path.to_string_lossy(),
        "time_s": time_s,
        "quality": "production",
        "width": frame.width,
        "height": frame.height,
        "native_adapter": {
            "vendor": adapter.vendor,
            "name": adapter.name,
            "backend": format!("{:?}", adapter.backend),
            "device_type": format!("{:?}", adapter.device_type),
            "driver": adapter.driver,
            "device": adapter.device,
        },
        "rgba_base64": base64::engine::general_purpose::STANDARD.encode(frame.rgba.as_slice()),
        "font_count": fonts.len(),
    });
    emit_json(&response)?;
    Ok(())
}

fn emit_json(value: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value)?;
    stdout.write_all(b"\n")?;
    Ok(())
}
