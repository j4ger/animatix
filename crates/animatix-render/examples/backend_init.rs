//! Repeatable native first-frame / steady completed-render benchmark.
//!
//! Run with `cargo run --release -p animatix-render --example backend_init -- [output.json] [frames]`.
//! The default is 3 fresh-renderer trials per scenario, 8 warmup frames, and 30
//! measured frames per trial. `frame0_completed_wall_ms` and steady frame times
//! use the blocking `OffscreenRenderer::render_timeline` API: they include CPU
//! submission, GPU execution, and mapped readback, and are not device timestamp
//! measurements. `renderer_new_wall_ms` is reported separately from frame 0.
//! Adapter identity comes from the actual renderer device. GPU memory is not
//! measured; host memory is never reported as VRAM.

use std::error::Error;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::time::Instant;

use animatix::timeline::{SceneDimensions, Timeline};
use animatix_render::offscreen::OffscreenRenderer;
use animatix_syntax::parser::parse_source;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const WARMUP_FRAMES: usize = 8;
const TRIALS: usize = 3;
const DEFAULT_MEASURED_FRAMES: usize = 30;
const MAX_MEASURED_FRAMES: usize = 120;
const FRAME_STEP_S: f64 = 1.0 / 60.0;

const PLAIN: &str = r#"
config { colorscheme: "editorial-dark", canvas_size: (1280, 720) }
background: Rect, at: (640, 360), size: (1280, 720), color: (0.04, 0.05, 0.08, 1), opacity: 1
stripe_a: Rect, at: (300, 360), size: (60, 680), color: (0.12, 0.55, 0.78, 1), opacity: 1
stripe_b: Rect, at: (470, 360), size: (60, 680), color: (0.85, 0.24, 0.2, 1), opacity: 1
stripe_c: Rect, at: (640, 360), size: (60, 680), color: (0.92, 0.72, 0.16, 1), opacity: 1
stripe_d: Rect, at: (810, 360), size: (60, 680), color: (0.25, 0.72, 0.42, 1), opacity: 1
stripe_e: Rect, at: (980, 360), size: (60, 680), color: (0.52, 0.35, 0.84, 1), opacity: 1
card: Rect, at: (640, 360), size: (600, 300), color: (0.96, 0.97, 0.99, 0.72), opacity: 1
"#;

const MASK: &str = r#"
config { colorscheme: "editorial-dark", canvas_size: (1280, 720) }
background: Rect, at: (640, 360), size: (1280, 720), color: (0.04, 0.05, 0.08, 1), opacity: 1
stripe_a: Rect, at: (300, 360), size: (60, 680), color: (0.12, 0.55, 0.78, 1), opacity: 1
stripe_b: Rect, at: (470, 360), size: (60, 680), color: (0.85, 0.24, 0.2, 1), opacity: 1
stripe_c: Rect, at: (640, 360), size: (60, 680), color: (0.92, 0.72, 0.16, 1), opacity: 1
stripe_d: Rect, at: (810, 360), size: (60, 680), color: (0.25, 0.72, 0.42, 1), opacity: 1
stripe_e: Rect, at: (980, 360), size: (60, 680), color: (0.52, 0.35, 0.84, 1), opacity: 1
clip: Mask, at: (640, 360), size: (600, 300), opacity: 1 {
  clip_shape: Ellipse, size: (600, 300), opacity: 1
  card: Rect, size: (600, 300), color: (0.96, 0.97, 0.99, 0.72), opacity: 1
}
"#;

// The stage is identity at t=0 and becomes active after the first frame. A
// backend predicate must account for the future keyframe so frame 0 can safely
// establish backend state before the Blur starts contributing.
const FUTURE_BLUR: &str = r#"
config { colorscheme: "editorial-dark", canvas_size: (1280, 720) }
background: Rect, at: (640, 360), size: (1280, 720), color: (0.04, 0.05, 0.08, 1), opacity: 1
stripe_a: Rect, at: (300, 360), size: (60, 680), color: (0.12, 0.55, 0.78, 1), opacity: 1
stripe_b: Rect, at: (470, 360), size: (60, 680), color: (0.85, 0.24, 0.2, 1), opacity: 1
stripe_c: Rect, at: (640, 360), size: (60, 680), color: (0.92, 0.72, 0.16, 1), opacity: 1
stripe_d: Rect, at: (810, 360), size: (60, 680), color: (0.25, 0.72, 0.42, 1), opacity: 1
stripe_e: Rect, at: (980, 360), size: (60, 680), color: (0.52, 0.35, 0.84, 1), opacity: 1
card: Filter, at: (640, 360), size: (600, 300), opacity: 1 {
  frost: Blur, radius: 0
  face: Rect, size: (600, 300), color: (0.96, 0.97, 0.99, 0.72), opacity: 1
}
#1s
card.frost.radius = 18 [500ms, ease: ease-out]
"#;

const ACTIVE_BLUR: &str = r#"
config { colorscheme: "editorial-dark", canvas_size: (1280, 720) }
background: Rect, at: (640, 360), size: (1280, 720), color: (0.04, 0.05, 0.08, 1), opacity: 1
stripe_a: Rect, at: (300, 360), size: (60, 680), color: (0.12, 0.55, 0.78, 1), opacity: 1
stripe_b: Rect, at: (470, 360), size: (60, 680), color: (0.85, 0.24, 0.2, 1), opacity: 1
stripe_c: Rect, at: (640, 360), size: (60, 680), color: (0.92, 0.72, 0.16, 1), opacity: 1
stripe_d: Rect, at: (810, 360), size: (60, 680), color: (0.25, 0.72, 0.42, 1), opacity: 1
stripe_e: Rect, at: (980, 360), size: (60, 680), color: (0.52, 0.35, 0.84, 1), opacity: 1
card: Filter, at: (640, 360), size: (600, 300), opacity: 1 {
  frost: Blur, radius: 18
  face: Rect, size: (600, 300), color: (0.96, 0.97, 0.99, 0.72), opacity: 1
}
"#;

const GLASS: &str = r#"
config { colorscheme: "editorial-dark", canvas_size: (1280, 720) }
background: Rect, at: (640, 360), size: (1280, 720), color: (0.04, 0.05, 0.08, 1), opacity: 1
stripe_a: Rect, at: (300, 360), size: (60, 680), color: (0.12, 0.55, 0.78, 1), opacity: 1
stripe_b: Rect, at: (470, 360), size: (60, 680), color: (0.85, 0.24, 0.2, 1), opacity: 1
stripe_c: Rect, at: (640, 360), size: (60, 680), color: (0.92, 0.72, 0.16, 1), opacity: 1
stripe_d: Rect, at: (810, 360), size: (60, 680), color: (0.25, 0.72, 0.42, 1), opacity: 1
stripe_e: Rect, at: (980, 360), size: (60, 680), color: (0.52, 0.35, 0.84, 1), opacity: 1
glass: Glass, at: (640, 360), size: (600, 300), corner_radius: 24, opacity: 1 {
  frost: Blur, radius: 18
  surface: Rect, size: (600, 300), color: (0.96, 0.97, 0.99, 0.72), opacity: 1
}
"#;

struct Scenario {
    name: &'static str,
    source: &'static str,
    frame0_time_s: f64,
    steady_start_s: f64,
}

const SCENARIOS: [Scenario; 5] = [
    Scenario {
        name: "plain",
        source: PLAIN,
        frame0_time_s: 0.0,
        steady_start_s: 0.5,
    },
    Scenario {
        name: "mask_clip_control",
        source: MASK,
        frame0_time_s: 0.0,
        steady_start_s: 0.5,
    },
    Scenario {
        name: "future_blur_identity_at_frame0",
        source: FUTURE_BLUR,
        frame0_time_s: 0.0,
        steady_start_s: 1.75,
    },
    Scenario {
        name: "active_filter_blur",
        source: ACTIVE_BLUR,
        frame0_time_s: 0.5,
        steady_start_s: 0.5,
    },
    Scenario {
        name: "active_glass_blur",
        source: GLASS,
        frame0_time_s: 0.5,
        steady_start_s: 0.5,
    },
];

#[derive(Clone)]
struct DeviceIdentity {
    name: String,
    vendor: u32,
    device: u32,
    device_type: String,
    pci_bus_id: String,
    driver: String,
    driver_info: String,
    backend: String,
}

struct TrialResult {
    trial: usize,
    renderer_new_wall_ms: f64,
    frame0_completed_wall_ms: f64,
    steady: Summary,
    adapter: DeviceIdentity,
}

#[derive(Clone, Copy)]
struct Summary {
    mean_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    min_ms: f64,
    max_ms: f64,
}

#[derive(Debug)]
struct MessageError(String);

impl std::fmt::Display for MessageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for MessageError {}

fn boxed_message(message: impl ToString) -> Box<dyn Error> {
    Box::new(MessageError(message.to_string()))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output_path = args.next().unwrap_or_else(|| "backend_init.json".to_string());
    let measured_frames = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(DEFAULT_MEASURED_FRAMES);
    if !(1..=MAX_MEASURED_FRAMES).contains(&measured_frames) || args.next().is_some() {
        return Err(boxed_message("usage: backend_init [output.json] [measured_frames: 1..=120]"));
    }

    let dimensions = SceneDimensions {
        width: WIDTH,
        height: HEIGHT,
    };
    let mut results: Vec<Vec<TrialResult>> = (0..SCENARIOS.len()).map(|_| Vec::new()).collect();

    // Rotate the order between fresh-device trials to reduce first-scenario
    // driver/cache bias while keeping every scenario's work fixed.
    for trial in 0..TRIALS {
        for offset in 0..SCENARIOS.len() {
            let scenario_index = (trial + offset) % SCENARIOS.len();
            let scenario = &SCENARIOS[scenario_index];
            let timeline = build_timeline(scenario.source)?;

            let renderer_started = Instant::now();
            let mut renderer = OffscreenRenderer::new().map_err(boxed_message)?;
            let renderer_new_wall_ms = elapsed_ms(renderer_started);
            let adapter = device_identity(renderer.device().adapter_info());

            let frame0_started = Instant::now();
            renderer
                .render_timeline(&timeline, scenario.frame0_time_s, dimensions)
                .map_err(boxed_message)?;
            let frame0_completed_wall_ms = elapsed_ms(frame0_started);

            for frame in 0..WARMUP_FRAMES {
                let time_s = scenario.steady_start_s + frame as f64 * FRAME_STEP_S;
                renderer.render_timeline(&timeline, time_s, dimensions).map_err(boxed_message)?;
            }

            let mut steady_samples_ms = Vec::with_capacity(measured_frames);
            for frame in 0..measured_frames {
                let time_s =
                    scenario.steady_start_s + (WARMUP_FRAMES + frame) as f64 * FRAME_STEP_S;
                let started = Instant::now();
                renderer.render_timeline(&timeline, time_s, dimensions).map_err(boxed_message)?;
                steady_samples_ms.push(elapsed_ms(started));
            }

            results[scenario_index].push(TrialResult {
                trial,
                renderer_new_wall_ms,
                frame0_completed_wall_ms,
                steady: summarize(&steady_samples_ms),
                adapter,
            });
        }
    }

    let json = to_json(&results, measured_frames);
    fs::write(output_path, json)?;
    Ok(())
}

fn build_timeline(source: &str) -> Result<Timeline, Box<dyn Error>> {
    let (ast, parse_errors) = parse_source(source);
    if !parse_errors.is_empty() {
        return Err(boxed_message(format!("benchmark fixture parse failed: {parse_errors:?}")));
    }
    let ast = ast.ok_or_else(|| boxed_message("benchmark fixture produced no AST"))?;
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    if !report.diagnostics.is_empty() {
        return Err(boxed_message(format!(
            "benchmark fixture build emitted diagnostics: {:?}",
            report.diagnostics
        )));
    }
    Ok(report.output)
}

fn device_identity(info: wgpu::AdapterInfo) -> DeviceIdentity {
    DeviceIdentity {
        name: info.name,
        vendor: info.vendor,
        device: info.device,
        device_type: format!("{:?}", info.device_type),
        pci_bus_id: info.device_pci_bus_id,
        driver: info.driver,
        driver_info: info.driver_info,
        backend: format!("{:?}", info.backend),
    }
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn summarize(values: &[f64]) -> Summary {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let median_ms = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };
    let p95_index = ((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    Summary {
        mean_ms: values.iter().sum::<f64>() / values.len() as f64,
        median_ms,
        p95_ms: sorted[p95_index.min(sorted.len() - 1)],
        min_ms: sorted[0],
        max_ms: sorted[sorted.len() - 1],
    }
}

fn to_json(results: &[Vec<TrialResult>], measured_frames: usize) -> String {
    let mut json = String::new();
    json.push_str("{\n  \"schema_version\": 1,\n  \"benchmark\": \"native_backend_init\",\n");
    let _ = writeln!(json, "  \"dimensions\": {{\"width\": {WIDTH}, \"height\": {HEIGHT}}},");
    let _ = writeln!(json, "  \"trials_per_scenario\": {TRIALS},");
    let _ = writeln!(json, "  \"warmup_frames_per_trial\": {WARMUP_FRAMES},");
    let _ = writeln!(json, "  \"measured_frames_per_trial\": {measured_frames},");
    json.push_str("  \"frame_timing_contract\": \"render_timeline blocks until readback completes; host wall time includes submission, GPU execution and readback, not GPU timestamp duration\",\n");
    json.push_str("  \"renderer_policy_scope\": \"native OffscreenRenderer API; use as a native initialization baseline, not browser WebGPU timing\",\n");
    json.push_str("  \"gpu_memory_measurement\": \"not measured\",\n");
    json.push_str("  \"scenarios\": [\n");

    for (scenario_index, trials) in results.iter().enumerate() {
        let scenario = &SCENARIOS[scenario_index];
        let trial_means: Vec<f64> = trials.iter().map(|trial| trial.steady.mean_ms).collect();
        let trial_medians: Vec<f64> = trials.iter().map(|trial| trial.steady.median_ms).collect();
        let trial_p95s: Vec<f64> = trials.iter().map(|trial| trial.steady.p95_ms).collect();
        let means = summarize(&trial_means);
        let medians = summarize(&trial_medians);
        let p95s = summarize(&trial_p95s);
        json.push_str("    {\n      \"name\": ");
        push_json_string(&mut json, scenario.name);
        let _ = writeln!(json, ",\n      \"frame0_time_s\": {},", scenario.frame0_time_s);
        let _ = writeln!(json, "      \"steady_start_time_s\": {},", scenario.steady_start_s);
        json.push_str("      \"steady_trial_summary_ms\": {\n");
        let _ = writeln!(json, "        \"mean_of_trial_means\": {:.6},", means.mean_ms);
        let _ = writeln!(json, "        \"median_of_trial_medians\": {:.6},", medians.median_ms);
        let _ = writeln!(json, "        \"median_of_trial_p95s\": {:.6},", p95s.median_ms);
        let _ = writeln!(json, "        \"min_trial_median\": {:.6},", medians.min_ms);
        let _ = writeln!(json, "        \"max_trial_median\": {:.6}\n      }},", medians.max_ms);
        json.push_str("      \"trials\": [\n");
        for (trial_index, trial) in trials.iter().enumerate() {
            json.push_str("        {\n");
            let _ = writeln!(json, "          \"trial\": {},", trial.trial);
            let _ = writeln!(
                json,
                "          \"renderer_new_wall_ms\": {:.6},",
                trial.renderer_new_wall_ms
            );
            let _ = writeln!(
                json,
                "          \"frame0_completed_wall_ms\": {:.6},",
                trial.frame0_completed_wall_ms
            );
            json.push_str("          \"steady_completed_wall_ms\": {\n");
            let _ = writeln!(json, "            \"mean\": {:.6},", trial.steady.mean_ms);
            let _ = writeln!(json, "            \"median\": {:.6},", trial.steady.median_ms);
            let _ = writeln!(json, "            \"p95\": {:.6},", trial.steady.p95_ms);
            let _ = writeln!(json, "            \"min\": {:.6},", trial.steady.min_ms);
            let _ =
                writeln!(json, "            \"max\": {:.6}\n          }},", trial.steady.max_ms);
            json.push_str("          \"adapter\": {\n            \"name\": ");
            push_json_string(&mut json, &trial.adapter.name);
            let _ = writeln!(json, ",\n            \"vendor\": {},", trial.adapter.vendor);
            let _ = writeln!(json, "            \"device\": {},", trial.adapter.device);
            json.push_str("            \"device_type\": ");
            push_json_string(&mut json, &trial.adapter.device_type);
            json.push_str(",\n            \"pci_bus_id\": ");
            push_json_string(&mut json, &trial.adapter.pci_bus_id);
            json.push_str(",\n            \"driver\": ");
            push_json_string(&mut json, &trial.adapter.driver);
            json.push_str(",\n            \"driver_info\": ");
            push_json_string(&mut json, &trial.adapter.driver_info);
            json.push_str(",\n            \"backend\": ");
            push_json_string(&mut json, &trial.adapter.backend);
            json.push_str("\n          }\n        }");
            if trial_index + 1 < trials.len() {
                json.push(',');
            }
            json.push('\n');
        }
        json.push_str("      ]\n    }");
        if scenario_index + 1 < results.len() {
            json.push(',');
        }
        json.push('\n');
    }

    json.push_str("  ]\n}\n");
    json
}

fn push_json_string(json: &mut String, value: &str) {
    json.push('"');
    for character in value.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            '\n' => json.push_str("\\n"),
            '\r' => json.push_str("\\r"),
            '\t' => json.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                let _ = write!(json, "\\u{:04x}", character as u32);
            },
            character => json.push(character),
        }
    }
    json.push('"');
}
