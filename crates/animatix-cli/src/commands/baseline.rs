//! `animatix baseline` command: visual regression snapshots and comparison.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::SceneDimensions;
use clap::Args as ClapArgs;
use tracing::error;

use crate::build::{
    configured_resolution, parse_set_pair, print_build_diagnostics, try_load_and_build,
};
use crate::diagnostics::OutputFormat;
use crate::extensions::CliExtensions;
use crate::frame_source::{
    DEFAULT_TOLERANCE, FrameView, differing_fraction, render_frame_observable,
};
use crate::review::SampleSpec;
use crate::review::samples::compute_sample_times;

/// CLI argument bundle for `animatix baseline`.
#[derive(ClapArgs, Debug)]
pub(crate) struct BaselineArgs {
    /// Path to the .amx file
    pub(crate) input: PathBuf,

    /// Update or generate baseline snapshots
    #[arg(long)]
    pub(crate) update: bool,

    /// Precise sample times in seconds (comma-separated, overrides auto keyframes)
    #[arg(long, value_delimiter = ',')]
    pub(crate) times: Option<Vec<f64>>,

    /// Maximum number of frames to sample when auto-detecting (default: 8)
    #[arg(long, default_value_t = 8)]
    pub(crate) max_frames: usize,

    /// Differing pixel ratio threshold (default: 0.001)
    #[arg(long, default_value_t = 0.001)]
    pub(crate) threshold: f64,

    /// Override canvas width (defaults to scene config resolution, else 1280)
    #[arg(long)]
    pub(crate) width: Option<u32>,

    /// Override canvas height (defaults to scene config resolution, else 720)
    #[arg(long)]
    pub(crate) height: Option<u32>,

    /// Output format (text or json)
    #[arg(long, default_value = "text")]
    pub(crate) format: OutputFormat,

    /// Override a build-time variable: `--set NAME=VALUE` (repeatable)
    #[arg(long = "set", value_parser = parse_set_pair)]
    pub(crate) set: Vec<(String, String)>,
}

/// Pure-Rust SHA-256 computation returning a lowercase 64-character hex digest.
/// Avoids external process dependencies while remaining deterministic across platforms.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let k: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut msg = bytes.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let (chunks, _) = msg.as_chunks::<64>();
    for chunk in chunks {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h_val) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 =
                h_val.wrapping_add(s1).wrapping_add(ch).wrapping_add(k[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            h_val = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(h_val);
    }
    let mut out = String::with_capacity(64);
    for val in h {
        let _ = write!(out, "{val:08x}");
    }
    out
}

#[derive(Debug)]
struct FrameComparison {
    time_s: f64,
    changed_fraction: f32,
    baseline_path: String,
    diff_image_path: Option<String>,
}

#[derive(Debug)]
struct BaselineOutcome {
    exit_code: i32,
    json_value: serde_json::Value,
    text_summary: String,
}

#[derive(Debug)]
struct ManifestData {
    frames: Vec<(usize, f64, String)>,
}

fn resolve_sample_times(
    target: &BuildTarget,
    times: Option<Vec<f64>>,
    max_frames: usize,
) -> Vec<f64> {
    let mut resolved = if let Some(times) = times {
        times
    } else {
        compute_sample_times(target, &SampleSpec::Keyframes, max_frames)
    };
    if resolved.is_empty() {
        resolved.push(0.0);
    }
    resolved.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    resolved.dedup_by(|a, b| (*a - *b).abs() < 0.0001);
    resolved
}

fn parse_and_validate_manifest(
    manifest_path: &Path,
    input: &Path,
    current_sha256: &str,
    current_res: (u32, u32),
    current_times: &[f64],
) -> Result<ManifestData, String> {
    if !manifest_path.is_file() {
        return Err(format!(
            "baseline manifest not found at {} — run with --update to generate baseline snapshots",
            manifest_path.display()
        ));
    }
    let content = std::fs::read_to_string(manifest_path)
        .map_err(|e| format!("cannot read manifest {}: {e}", manifest_path.display()))?;
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("corrupt manifest JSON in {}: {e}", manifest_path.display()))?;

    let source_sha = json
        .get("source_sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "manifest missing 'source_sha256'".to_string())?;
    if source_sha != current_sha256 {
        return Err(format!(
            "source changed for {} (baseline sha256: {source_sha}, current: {current_sha256}) — run with --update after reviewing the diff",
            input.display()
        ));
    }

    let res_arr = json
        .get("resolution")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "manifest missing 'resolution'".to_string())?;
    if res_arr.len() != 2 {
        return Err("invalid 'resolution' in manifest".to_string());
    }
    let m_w = res_arr[0].as_u64().unwrap_or(0) as u32;
    let m_h = res_arr[1].as_u64().unwrap_or(0) as u32;
    if (m_w, m_h) != current_res {
        return Err(format!(
            "resolution mismatch for {}: baseline was {m_w}x{m_h}, current is {}x{} — run with --update to record new resolution",
            input.display(),
            current_res.0,
            current_res.1,
        ));
    }

    let times_arr = json
        .get("times")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "manifest missing 'times'".to_string())?;
    let m_times: Vec<f64> = times_arr.iter().filter_map(|v| v.as_f64()).collect();
    if m_times.len() != current_times.len()
        || !m_times.iter().zip(current_times).all(|(a, b)| (a - b).abs() < 0.001)
    {
        return Err(format!(
            "sample times mismatch for {}: baseline had {m_times:?}, current has {current_times:?} — run with --update to record new sample times",
            input.display()
        ));
    }

    let frames_arr = json
        .get("frames")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "manifest missing 'frames'".to_string())?;
    let mut frames = Vec::with_capacity(frames_arr.len());
    for item in frames_arr {
        let idx =
            item.get("index")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "manifest frame missing 'index'".to_string())? as usize;
        let t = item
            .get("time_s")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| "manifest frame missing 'time_s'".to_string())?;
        let p = item
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "manifest frame missing 'path'".to_string())?;
        frames.push((idx, t, p.to_string()));
    }

    Ok(ManifestData { frames })
}

fn pixel_channel_differs(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a[..3].iter().zip(&b[..3]).any(|(x, y)| x.abs_diff(*y) > tol)
}

fn update_baseline(
    renderer: &mut animatix_render::offscreen::OffscreenRenderer,
    target: &BuildTarget,
    dimensions: SceneDimensions,
    sample_times: &[f64],
    snapshots_dir: &Path,
    stem: &str,
    source_sha256: &str,
    threshold: f64,
) -> Result<BaselineOutcome, String> {
    std::fs::create_dir_all(snapshots_dir).map_err(|e| {
        format!("failed to create baseline directory {}: {e}", snapshots_dir.display())
    })?;

    let mut frames_meta = Vec::with_capacity(sample_times.len());
    let mut json_frames = Vec::with_capacity(sample_times.len());

    for (index, &time_s) in sample_times.iter().enumerate() {
        let obs = render_frame_observable(renderer, target, dimensions, time_s)
            .map_err(|e| format!("failed to render frame at {time_s:.3}s: {e}"))?;

        let filename = format!("frame_{index:04}_{time_s:.3}s.png");
        let png_path = snapshots_dir.join(&filename);
        let rel_path = format!("snapshots/{stem}/{filename}");

        let img = image::RgbaImage::from_raw(
            dimensions.width,
            dimensions.height,
            obs.frame.rgba.to_vec(),
        )
        .ok_or_else(|| format!("invalid frame buffer size for {filename}"))?;

        img.save(&png_path)
            .map_err(|e| format!("failed to save snapshot {}: {e}", png_path.display()))?;

        // Remove any stale diff image
        let diff_filename = format!("frame_{index:04}_{time_s:.3}s.diff.png");
        let _ = std::fs::remove_file(snapshots_dir.join(diff_filename));

        frames_meta.push(serde_json::json!({
            "index": index,
            "time_s": time_s,
            "path": rel_path,
        }));

        json_frames.push(serde_json::json!({
            "time_s": time_s,
            "changed_fraction": 0.0,
            "baseline": rel_path,
        }));
    }

    let manifest_val = serde_json::json!({
        "source_sha256": source_sha256,
        "times": sample_times,
        "resolution": [dimensions.width, dimensions.height],
        "threshold": threshold,
        "frames": frames_meta,
    });

    let manifest_path = snapshots_dir.join("manifest.json");
    let manifest_str = serde_json::to_string_pretty(&manifest_val)
        .map_err(|e| format!("failed to serialize manifest: {e}"))?;
    std::fs::write(&manifest_path, manifest_str)
        .map_err(|e| format!("failed to write manifest {}: {e}", manifest_path.display()))?;

    let total = sample_times.len();
    let json_value = serde_json::json!({
        "kind": "baseline",
        "verdict": "updated",
        "frames": json_frames,
        "summary": {
            "total_frames": total,
            "updated_frames": total,
            "threshold": threshold,
        }
    });

    let text_summary = format!(
        "Updated baseline for {stem} ({total} frames saved to {})",
        snapshots_dir.display()
    );

    Ok(BaselineOutcome {
        exit_code: 0,
        json_value,
        text_summary,
    })
}

fn compare_baseline(
    renderer: &mut animatix_render::offscreen::OffscreenRenderer,
    target: &BuildTarget,
    dimensions: SceneDimensions,
    sample_times: &[f64],
    snapshots_dir: &Path,
    stem: &str,
    manifest: &ManifestData,
    threshold: f64,
    input_display: &str,
) -> Result<BaselineOutcome, String> {
    let mut comparisons = Vec::with_capacity(sample_times.len());
    let mut differing_count = 0usize;
    let tol_u8 = (DEFAULT_TOLERANCE.clamp(0.0, 1.0) * 255.0).round() as u8;

    for (index, &time_s) in sample_times.iter().enumerate() {
        let obs = render_frame_observable(renderer, target, dimensions, time_s)
            .map_err(|e| format!("failed to render frame at {time_s:.3}s: {e}"))?;

        let manifest_frame = manifest
            .frames
            .iter()
            .find(|(i, _, _)| *i == index)
            .ok_or_else(|| format!("frame {index} not found in manifest"))?;
        let base_rel_path = &manifest_frame.2;

        let base_full_path = {
            let p = Path::new(base_rel_path);
            if p.is_file() {
                p.to_path_buf()
            } else {
                snapshots_dir.join(p.file_name().unwrap_or(p.as_os_str()))
            }
        };

        if !base_full_path.is_file() {
            return Err(format!(
                "baseline snapshot not found at {} — run with --update to regenerate",
                base_full_path.display()
            ));
        }

        let base_img = image::open(&base_full_path)
            .map_err(|e| format!("cannot open baseline image {}: {e}", base_full_path.display()))?
            .to_rgba8();

        if base_img.width() != dimensions.width || base_img.height() != dimensions.height {
            return Err(format!(
                "baseline image {} dimensions {}x{} do not match scene {}x{}",
                base_full_path.display(),
                base_img.width(),
                base_img.height(),
                dimensions.width,
                dimensions.height
            ));
        }

        let cur_view = FrameView::new(dimensions.width, dimensions.height, &obs.frame.rgba)
            .ok_or_else(|| "invalid current frame buffer".to_string())?;
        let base_view = FrameView::new(dimensions.width, dimensions.height, base_img.as_raw())
            .ok_or_else(|| "invalid baseline frame buffer".to_string())?;

        let changed_fraction = differing_fraction(&cur_view, &base_view, DEFAULT_TOLERANCE);
        let has_diff = (changed_fraction as f64) > threshold;

        let diff_filename = format!("frame_{index:04}_{time_s:.3}s.diff.png");
        let diff_path = snapshots_dir.join(&diff_filename);
        let diff_rel_path = format!("snapshots/{stem}/{diff_filename}");

        if has_diff {
            differing_count += 1;
            let mut diff_img = image::RgbaImage::new(dimensions.width, dimensions.height);
            for y in 0..dimensions.height {
                for x in 0..dimensions.width {
                    let cur_p = cur_view.pixel(x, y);
                    let base_p = base_view.pixel(x, y);
                    if pixel_channel_differs(cur_p, base_p, tol_u8) {
                        diff_img.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
                    } else {
                        diff_img.put_pixel(x, y, image::Rgba(cur_p));
                    }
                }
            }
            diff_img
                .save(&diff_path)
                .map_err(|e| format!("failed to save diff image {}: {e}", diff_path.display()))?;

            comparisons.push(FrameComparison {
                time_s,
                changed_fraction,
                baseline_path: base_rel_path.clone(),
                diff_image_path: Some(diff_rel_path),
            });
        } else {
            let _ = std::fs::remove_file(&diff_path);
            comparisons.push(FrameComparison {
                time_s,
                changed_fraction,
                baseline_path: base_rel_path.clone(),
                diff_image_path: None,
            });
        }
    }

    let total = sample_times.len();
    let passed = differing_count == 0;

    let mut json_frames = Vec::with_capacity(total);
    for c in &comparisons {
        let mut obj = serde_json::json!({
            "time_s": c.time_s,
            "changed_fraction": c.changed_fraction,
            "baseline": c.baseline_path,
        });
        if let Some(ref diff) = c.diff_image_path {
            obj["diff_image"] = serde_json::Value::String(diff.clone());
        }
        json_frames.push(obj);
    }

    let json_value = serde_json::json!({
        "kind": "baseline",
        "verdict": if passed { "passed" } else { "failed" },
        "schema_version": 3,
        "frames": json_frames,
        "summary": {
            "total_frames": total,
            "differing_frames": differing_count,
            "threshold": threshold,
        }
    });

    let mut text_summary = String::new();
    if passed {
        text_summary = format!(
            "Baseline check PASSED for {input_display} ({total} frames checked, 0 differing, threshold: {threshold})"
        );
    } else {
        text_summary.push_str(&format!("Baseline check FAILED for {input_display}:\n"));
        for c in &comparisons {
            if let Some(ref diff_img) = c.diff_image_path {
                text_summary.push_str(&format!(
                    "  time {:.3}s: changed fraction {:.4} > threshold {:.4} (diff: {diff_img})\n",
                    c.time_s, c.changed_fraction, threshold
                ));
            }
        }
        text_summary.push_str(&format!(
            "{total} frame(s) checked: {differing_count} differing (threshold: {threshold})\nRun `animatix baseline {input_display} --update` after reviewing the diff to accept these changes."
        ));
    }

    Ok(BaselineOutcome {
        exit_code: if passed { 0 } else { 1 },
        json_value,
        text_summary,
    })
}

fn execute_baseline(
    args: BaselineArgs,
    extensions: &CliExtensions,
) -> Result<BaselineOutcome, String> {
    let BaselineArgs {
        input,
        update,
        times,
        max_frames,
        threshold,
        width,
        height,
        format: _,
        set,
    } = args;

    if !input.is_file() {
        return Err(format!("source file does not exist: {}", input.display()));
    }

    let source_bytes = std::fs::read(&input)
        .map_err(|e| format!("cannot read source file {}: {e}", input.display()))?;
    let source_sha256 = sha256_hex(&source_bytes);

    let (target, diagnostics) = try_load_and_build(&input, extensions, &set)
        .map_err(|e| format!("cannot load scene {}: {e}", input.display()))?;
    if diagnostics.iter().any(|d| d.is_error()) {
        print_build_diagnostics(&diagnostics);
        let error_count = diagnostics.iter().filter(|d| d.is_error()).count();
        return Err(format!("build produced {error_count} error diagnostic(s)"));
    }

    let (default_w, default_h) = configured_resolution(&target).unwrap_or((1280, 720));
    let scene_width = width.unwrap_or(default_w);
    let scene_height = height.unwrap_or(default_h);
    let dimensions = SceneDimensions {
        width: scene_width,
        height: scene_height,
    };

    let sample_times = resolve_sample_times(&target, times, max_frames);

    let mut renderer = animatix_render::offscreen::OffscreenRenderer::new()
        .map_err(|e| format!("no rendering adapter available: {e}"))?;

    // Key on the input path (relative to cwd, extension removed), not just the
    // file stem: several dogfood projects are all named `entry.amx` and would
    // otherwise overwrite one shared snapshot directory.
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let input_rel = input.strip_prefix(&cwd).unwrap_or(&input);
    let mut key_path = input_rel.to_path_buf();
    key_path.set_extension("");
    let snapshot_key = key_path.to_string_lossy().replace('\\', "/");
    let snapshots_dir = PathBuf::from("snapshots").join(&snapshot_key);
    let manifest_path = snapshots_dir.join("manifest.json");

    if update {
        update_baseline(
            &mut renderer,
            &target,
            dimensions,
            &sample_times,
            &snapshots_dir,
            &snapshot_key,
            &source_sha256,
            threshold,
        )
    } else {
        let manifest = parse_and_validate_manifest(
            &manifest_path,
            &input,
            &source_sha256,
            (scene_width, scene_height),
            &sample_times,
        )?;

        compare_baseline(
            &mut renderer,
            &target,
            dimensions,
            &sample_times,
            &snapshots_dir,
            &snapshot_key,
            &manifest,
            threshold,
            &input.display().to_string(),
        )
    }
}

/// Run one baseline command and own its exit code.
pub(crate) fn run(args: BaselineArgs, extensions: &CliExtensions) {
    let format = args.format;
    match execute_baseline(args, extensions) {
        Ok(outcome) => {
            match format {
                OutputFormat::Json => println!("{}", outcome.json_value),
                OutputFormat::Text => println!("{}", outcome.text_summary),
            }
            std::process::exit(outcome.exit_code);
        },
        Err(e) => {
            if format == OutputFormat::Json {
                println!(
                    "{}",
                    serde_json::json!({
                        "kind": "baseline",
                        "verdict": "error",
                        "error": e,
                    })
                );
            }
            error!("{e}");
            std::process::exit(2);
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser, Debug)]
    struct TestCli {
        #[command(subcommand)]
        command: TestSubcommands,
    }

    #[derive(clap::Subcommand, Debug)]
    enum TestSubcommands {
        Baseline {
            #[command(flatten)]
            args: BaselineArgs,
        },
    }

    #[test]
    fn baseline_args_defaults() {
        let cli = TestCli::try_parse_from(["animatix", "baseline", "scene.amx"]).unwrap();
        let TestSubcommands::Baseline { args } = cli.command;
        assert_eq!(args.input, PathBuf::from("scene.amx"));
        assert!(!args.update);
        assert_eq!(args.times, None);
        assert_eq!(args.max_frames, 8);
        assert_eq!(args.threshold, 0.001);
        assert_eq!(args.width, None);
        assert_eq!(args.height, None);
        assert_eq!(args.format, OutputFormat::Text);
        assert!(args.set.is_empty());
    }

    #[test]
    fn baseline_args_custom() {
        let cli = TestCli::try_parse_from([
            "animatix",
            "baseline",
            "scene.amx",
            "--update",
            "--times",
            "0.0,0.5,1.25",
            "--max-frames",
            "16",
            "--threshold",
            "0.005",
            "--width",
            "1920",
            "--height",
            "1080",
            "--format",
            "json",
            "--set",
            "title=42",
        ])
        .unwrap();
        let TestSubcommands::Baseline { args } = cli.command;
        assert_eq!(args.input, PathBuf::from("scene.amx"));
        assert!(args.update);
        assert_eq!(args.times, Some(vec![0.0, 0.5, 1.25]));
        assert_eq!(args.max_frames, 16);
        assert_eq!(args.threshold, 0.005);
        assert_eq!(args.width, Some(1920));
        assert_eq!(args.height, Some(1080));
        assert_eq!(args.format, OutputFormat::Json);
        assert_eq!(args.set, vec![("title".to_string(), "42".to_string())]);
    }

    #[test]
    fn test_sha256_known_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_pixel_channel_differs() {
        let p1 = [100, 150, 200, 255];
        let p2 = [102, 149, 201, 255];
        let p3 = [110, 150, 200, 255];
        let tol = 5; // tolerance in u8
        assert!(!pixel_channel_differs(p1, p2, tol));
        assert!(pixel_channel_differs(p1, p3, tol));
    }

    #[test]
    fn test_manifest_validation() {
        let dir = std::env::temp_dir().join(format!("baseline_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let manifest_path = dir.join("manifest.json");

        let valid_json = serde_json::json!({
            "source_sha256": "abcdef123456",
            "resolution": [1280, 720],
            "times": [0.0, 1.0],
            "threshold": 0.001,
            "frames": [
                {"index": 0, "time_s": 0.0, "path": "frame_0000_0.000s.png"},
                {"index": 1, "time_s": 1.0, "path": "frame_0001_1.000s.png"}
            ]
        });
        std::fs::write(&manifest_path, valid_json.to_string()).unwrap();

        let input = Path::new("dummy.amx");

        // Valid case
        let res = parse_and_validate_manifest(
            &manifest_path,
            input,
            "abcdef123456",
            (1280, 720),
            &[0.0, 1.0],
        );
        assert!(res.is_ok());
        let data = res.unwrap();
        assert_eq!(data.frames.len(), 2);

        // SHA mismatch
        let err_sha = parse_and_validate_manifest(
            &manifest_path,
            input,
            "different_sha",
            (1280, 720),
            &[0.0, 1.0],
        )
        .unwrap_err();
        assert!(err_sha.contains("source changed"));

        // Resolution mismatch
        let err_res = parse_and_validate_manifest(
            &manifest_path,
            input,
            "abcdef123456",
            (1920, 1080),
            &[0.0, 1.0],
        )
        .unwrap_err();
        assert!(err_res.contains("resolution mismatch"));

        // Times mismatch
        let err_times = parse_and_validate_manifest(
            &manifest_path,
            input,
            "abcdef123456",
            (1280, 720),
            &[0.0, 0.5, 1.0],
        )
        .unwrap_err();
        assert!(err_times.contains("sample times mismatch"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
