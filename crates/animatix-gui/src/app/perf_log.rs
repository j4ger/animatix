//! JSONL performance sink for real-authoring telemetry (PF-9).
//!
//! Opt-in via `animatix-gui --perf-log <path>.jsonl`. One JSON line is appended
//! per UI frame with the HUD metrics (`PerformanceMetrics`) plus the per-stage
//! durations drained from the shared stage tracer (`animatix::perf`, PF-8) so
//! collected data uses the exact same stage names as the bench suite.
//!
//! Threading note: the stage tracer is thread-local. The GUI drains it on the
//! UI thread, so it observes the stages executed there (frame env, sampling,
//! modifier exec, layout, raster). The `rebuild` stage runs on the rebuild
//! worker thread and is instead covered by the top-level `rebuild_ms` field,
//! which is the worker-reported end-to-end rebuild time.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::preview::performance::PerformanceMetrics;

/// Append-only JSONL sink for per-frame performance telemetry.
#[derive(Debug)]
pub struct PerfLogSink {
    file: Option<std::fs::File>,
    /// Original sink path; retained for diagnostics messages.
    _path: PathBuf,
}

impl PerfLogSink {
    /// Open (create or append) the JSONL sink at `path`.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Some(file),
            _path: path.to_path_buf(),
        })
    }

    /// Append one JSON line for the current UI frame and flush it.
    ///
    /// `stage_measurements` comes from `animatix::perf::take_measurements()`
    /// (drained by the caller). On the first I/O error the sink disables
    /// itself and warns — a broken telemetry file must never take the GUI
    /// down or spam per-frame warnings.
    pub fn record_frame(
        &mut self,
        metrics: &PerformanceMetrics,
        stage_measurements: Vec<(String, Duration)>,
        actor_count: usize,
        scene_dimensions: [f64; 2],
    ) {
        let Some(file) = self.file.as_mut() else {
            return; // Sink disabled after an earlier I/O error; stay silent.
        };
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // Clock before epoch: emit 0 rather than failing the frame.
        let stages: serde_json::Map<String, serde_json::Value> = stage_measurements
            .into_iter()
            .map(|(name, d)| (name, serde_json::Value::from(d.as_secs_f64() * 1000.0)))
            .collect();
        let line = serde_json::json!({
            "ts": ts,
            "fps": metrics.fps,
            "rebuild_ms": metrics.rebuild_time_ms,
            "render_ms": metrics.render_time_ms,
            "stale": metrics.is_stale,
            "actors": actor_count,
            "scene_size": [scene_dimensions[0], scene_dimensions[1]],
            "stages": stages,
        });
        let result = writeln!(file, "{line}").and_then(|()| file.flush());
        if let Err(e) = result {
            self.file = None;
            tracing::warn!("perf-log sink disabled after I/O error: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metrics() -> PerformanceMetrics {
        let mut metrics = PerformanceMetrics::new();
        metrics.record_tick();
        metrics.record_rebuild(1.5);
        metrics.record_render(4.25);
        metrics
    }

    #[test]
    fn writes_one_parseable_line_per_frame() {
        let dir =
            std::env::temp_dir().join(format!("animatix-perf-log-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("perf.jsonl");
        let _ = std::fs::remove_file(&path);

        {
            let mut sink = PerfLogSink::open(&path).unwrap();
            let stages = vec![("build_frame_env".to_string(), Duration::from_micros(120))];
            sink.record_frame(&sample_metrics(), stages, 7, [1280.0, 720.0]);
            sink.record_frame(&sample_metrics(), Vec::new(), 7, [1280.0, 720.0]);
        }

        let content = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "one JSON line per recorded frame");

        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["rebuild_ms"], 1.5);
        assert_eq!(first["render_ms"], 4.25);
        assert_eq!(first["actors"], 7);
        assert_eq!(first["scene_size"][0], 1280.0);
        assert_eq!(first["stages"]["build_frame_env"], 0.12);
        assert!(first["ts"].as_u64().is_some(), "timestamp must be present");

        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert!(second["stages"].as_object().unwrap().is_empty());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn disables_itself_after_io_error() {
        // Opening a sink on a directory succeeds on some platforms but every
        // write fails; either way record_frame must not panic and must stop
        // attempting I/O after the first failure.
        let dir =
            std::env::temp_dir().join(format!("animatix-perf-log-err-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not-writable");
        std::fs::create_dir(&path).unwrap(); // A directory as the log target.

        if let Ok(mut sink) = PerfLogSink::open(&path) {
            sink.record_frame(&sample_metrics(), Vec::new(), 0, [0.0, 0.0]);
            sink.record_frame(&sample_metrics(), Vec::new(), 0, [0.0, 0.0]);
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
