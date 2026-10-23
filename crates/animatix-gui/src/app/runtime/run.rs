use super::*;

pub fn run_gui(path: Option<PathBuf>, perf_log_path: Option<PathBuf>, script: Option<PathBuf>) {
    // Open the optional JSONL perf sink (PF-9) before the event loop starts; a
    // bad path must not take the GUI down — log and continue without telemetry.
    let perf_log = perf_log_path.and_then(|p| {
        crate::app::perf_log::PerfLogSink::open(&p)
            .map_err(|e| tracing::error!("--perf-log: cannot open {}: {e}", p.display()))
            .ok()
    });
    let (initial_path, show_welcome) = match path {
        Some(p) => (p, false),
        None => match load_app_state() {
            Some(p) => (p, false),
            None => (default_file_path(), true),
        },
    };

    // Load persisted window geometry before constructing NativeOptions
    let persistence_path = persistence_path();
    let persistence = load_workspace_persistence(&persistence_path);
    let window_size = persistence.as_ref().and_then(|p| p.window_size);
    let window_maximized = persistence.as_ref().and_then(|p| p.window_maximized).unwrap_or(false);

    let viewport = egui::ViewportBuilder::default()
        .with_title("Animatix")
        .with_inner_size(
            window_size
                .map(|[w, h]| egui::Vec2::new(w, h))
                .unwrap_or(egui::vec2(INITIAL_WINDOW_SIZE.0 as f32, INITIAL_WINDOW_SIZE.1 as f32)),
        )
        .with_maximized(window_maximized);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Animatix",
        options,
        Box::new(move |cc| {
            let app = AnimatixApp::new(cc, initial_path, show_welcome, perf_log)?;
            // Demo-script driver (PF-11 follow-up): inject commands from a
            // script at fixed wall-clock times — the scripted form of the
            // external-command queue, used to drive scripted seek/playback
            // sessions for `--perf-log` capture.
            if let Some(script) = script {
                let sender = app.shell.external_command_sender();
                let ctx = cc.egui_ctx.clone();
                std::thread::spawn(move || {
                    let script = match std::fs::read_to_string(&script) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("demo script: {e}");
                            return;
                        },
                    };
                    let start = std::time::Instant::now();
                    ctx.request_repaint();
                    for line in script.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        // Grammar: `<seconds> scrub <t> | <seconds> play | <seconds> pause`
                        let Some((at_s, action)) = line.split_once(char::is_whitespace) else {
                            continue;
                        };
                        let at: f64 = match at_s.parse() {
                            Ok(v) => v,
                            Err(_) => {
                                tracing::warn!("demo script: bad time '{at_s}'");
                                continue;
                            },
                        };
                        let wait = (at - start.elapsed().as_secs_f64()).max(0.0);
                        std::thread::sleep(std::time::Duration::from_secs_f64(wait));
                        let result = match action.trim() {
                            "play" => {
                                sender.send_command(crate::app::commands::Command::TogglePlayback)
                            },
                            "pause" => {
                                sender.send_command(crate::app::commands::Command::TogglePlayback)
                            },
                            other if other.starts_with("scrub ") => {
                                match other["scrub ".len()..].trim().parse::<f64>() {
                                    Ok(t) => sender
                                        .send_command(crate::app::commands::Command::ScrubTo(t)),
                                    Err(_) => {
                                        tracing::warn!("demo script: bad scrub time '{other}'");
                                        Ok(())
                                    },
                                }
                            },
                            other => {
                                tracing::warn!("demo script: unknown action '{other}'");
                                Ok(())
                            },
                        };
                        if result.is_err() {
                            break; // App closed.
                        }
                        // The event loop idles without a repaint request — the
                        // queued command would never be drained.
                        ctx.request_repaint();
                    }
                });
            }
            Ok(Box::new(app))
        }),
    ) {
        tracing::error!("Failed to run Animatix GUI: {e}");
        std::process::exit(1);
    }
}
