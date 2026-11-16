//! Manual Q=1 completed-frame driver. Use tools wrapper for timeout/provenance.
#[path = "offscreen_blur_area/workload.rs"]
mod workload;

use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use animatix_render::offscreen::OffscreenRenderer;
use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Instant;

fn quoted(value: &str) -> String {
    let mut output = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            c if c.is_control() => {
                let _ = write!(output, "\\u{:04x}", c as u32);
            },
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 || (args.len() == 3 && args[2] != "--allow-software") {
        return Err(
            "usage: offscreen_blur_area output.json frames[1..120] [--allow-software]".into()
        );
    }
    let frames: usize = args[1].parse()?;
    if !(1..=120).contains(&frames) {
        return Err("frames must be 1..120".into());
    }
    // Reserve before work; never overwrite a previous result. On failure this
    // remains empty, and the wrapper records a failed status, never success.
    let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(&args[0])?;
    let mut renderer = OffscreenRenderer::new()?;
    let info = renderer.device().adapter_info();
    let class = workload::device_class(info.device_type);
    if class == "software" && args.len() != 3 {
        return Err("software adapter refused; use --allow-software for separately labelled diagnostic data".into());
    }
    let dims = SceneDimensions {
        width: workload::WIDTH,
        height: workload::WIDTH,
    };
    let mut rows = Vec::new();
    for size in workload::SIZES {
        for blur in [false, true] {
            let source = workload::source(size, blur);
            let timeline = workload::build(&source)?;
            for _ in 0..workload::WARMUP {
                let pending = renderer.begin_frame_with_debug(
                    &timeline,
                    0.0,
                    dims,
                    DebugRenderOptions::default(),
                )?;
                let frame = renderer.wait_frame(pending)?;
                workload::guard(&frame, size, blur)?;
            }
            let mut samples = Vec::new();
            for _ in 0..frames {
                let start = Instant::now();
                let pending = renderer.begin_frame_with_debug(
                    &timeline,
                    0.0,
                    dims,
                    DebugRenderOptions::default(),
                )?;
                let submitted = Instant::now();
                let frame = renderer.wait_frame(pending)?;
                let completed = Instant::now();
                workload::guard(&frame, size, blur)?;
                samples.push(format!(
                    "[{}, {}, {}]",
                    submitted.duration_since(start).as_nanos(),
                    completed.duration_since(submitted).as_nanos(),
                    completed.duration_since(start).as_nanos()
                ));
                drop(frame);
            }
            rows.push(format!(
                "{{\"size\":{size},\"mode\":{},\"source\":{},\"samples_ns\":[{}]}}",
                quoted(if blur { "blur" } else { "plain" }),
                quoted(&source),
                samples.join(",")
            ));
        }
    }
    let json = format!(
        "{{\"schema\":1,\"workload\":\"offscreen_blur_area_v1\",\"width\":512,\"height\":512,\"radius\":{},\"queue_depth\":1,\"warmup\":{},\"class\":{},\"adapter\":{{\"name\":{},\"vendor\":{},\"device\":{},\"device_type\":{},\"backend\":{},\"driver\":{},\"driver_info\":{}}},\"columns\":[\"host_evaluate_encode_submit_ns\",\"completion_map_cpu_copy_ns\",\"total_completed_frame_ns\"],\"cases\":[{}]}}\n",
        workload::RADIUS,
        workload::WARMUP,
        quoted(class),
        quoted(&info.name),
        info.vendor,
        info.device,
        quoted(&format!("{:?}", info.device_type)),
        quoted(&format!("{:?}", info.backend)),
        quoted(&info.driver),
        quoted(&info.driver_info),
        rows.join(",")
    );
    output.write_all(json.as_bytes())?;
    Ok(())
}
