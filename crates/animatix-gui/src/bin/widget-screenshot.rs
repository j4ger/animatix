//! Bounded widget screenshot binary.
//!
//! Renders isolated theme-aware eparts surfaces and saves one PNG, then exits.
//! It is meant to be run under a timeout or xvfb when no display is available:
//!
//! ```bash
//! timeout 30s cargo run --features dev-screenshots --bin widget-screenshot -- \
//!   --widget overview --output /tmp/gui.png --theme light
//! ```

use std::path::PathBuf;

use animatix_gui::dev::screenshot_harness::{WIDGET_REGISTRY, install_theme, render_widget};
use eframe::egui;
use eparts::Theme;

struct Args {
    widget: String,
    output: PathBuf,
    width: u32,
    height: u32,
    list: bool,
    dark: bool,
}

fn parse_args() -> Args {
    let mut args = std::env::args().skip(1);
    let mut widget = String::new();
    let mut output = PathBuf::from("/tmp/animatix_screenshots/widget.png");
    let mut width = 520u32;
    let mut height = 320u32;
    let mut list = false;
    let mut dark = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--widget" | "-w" => widget = args.next().unwrap_or_default(),
            "--output" | "-o" => output = PathBuf::from(args.next().unwrap_or_default()),
            "--width" => width = args.next().and_then(|s| s.parse().ok()).unwrap_or(520),
            "--height" => height = args.next().and_then(|s| s.parse().ok()).unwrap_or(320),
            "--theme" => {
                let value = args.next().unwrap_or_default();
                dark = !matches!(value.as_str(), "light");
            },
            "--list" | "-l" => list = true,
            _ => {},
        }
    }

    Args {
        widget,
        output,
        width,
        height,
        list,
        dark,
    }
}

struct ScreenshotApp {
    widget: String,
    output: PathBuf,
    frame_count: u32,
    done: bool,
    theme: Theme,
    dark: bool,
}

impl ScreenshotApp {
    fn new(widget: String, output: PathBuf, dark: bool) -> Self {
        let theme = if dark { Theme::dark() } else { Theme::light() };
        Self {
            widget,
            output,
            frame_count: 0,
            done: false,
            theme,
            dark,
        }
    }
}

impl eframe::App for ScreenshotApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_count += 1;

        if self.frame_count == 1 {
            install_theme(ui.ctx(), self.theme, self.dark);
        }

        let theme = eparts::theme(ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme.surface.base))
            .show_inside(ui, |ui| {
                let content_rect = ui.available_rect_before_wrap().shrink(20.0);
                ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
                    render_widget(ui, &self.widget, theme);
                });
            });

        if self.frame_count == 2 {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }

        if self.frame_count >= 2 {
            ui.input(|i| {
                for event in &i.raw.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        if let Err(e) = save_screenshot(image, i.pixels_per_point, &self.output) {
                            eprintln!("Failed to save screenshot: {e}");
                            std::process::exit(1);
                        }
                        println!("Screenshot saved to {}", self.output.display());
                        self.done = true;
                    }
                }
            });
        }

        if self.done {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ui.ctx().request_repaint();
        }
    }
}

fn save_screenshot(
    image: &egui::ColorImage,
    pixels_per_point: f32,
    path: &PathBuf,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }

    let [img_w, img_h] = image.size;
    let logical_w = (img_w as f32 / pixels_per_point).round() as u32;
    let logical_h = (img_h as f32 / pixels_per_point).round() as u32;

    let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let src = image::RgbaImage::from_raw(img_w as u32, img_h as u32, raw)
        .ok_or("invalid screenshot buffer")?;
    let resized = image::imageops::resize(
        &src,
        logical_w.max(1),
        logical_h.max(1),
        image::imageops::FilterType::Lanczos3,
    );

    resized.save(path).map_err(|e| format!("save png: {e}"))?;
    Ok(())
}

fn main() {
    let args = parse_args();

    if args.list {
        println!("Available widgets:");
        for (id, desc) in WIDGET_REGISTRY {
            println!("  {:24} — {desc}", id);
        }
        return;
    }

    if args.widget.is_empty() {
        eprintln!(
            "Usage: widget-screenshot --widget <name> [--output <path>] [--width <px>] [--height <px>] [--theme dark|light]"
        );
        eprintln!("       widget-screenshot --list");
        std::process::exit(1);
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(format!("Widget Screenshot: {}", args.widget))
            .with_inner_size(egui::vec2(args.width as f32, args.height as f32))
            .with_decorations(false),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Widget Screenshot",
        options,
        Box::new(|_cc| {
            let app = ScreenshotApp::new(args.widget.clone(), args.output.clone(), args.dark);
            Ok(Box::new(app))
        }),
    ) {
        eprintln!("Failed to run widget screenshot: {e}");
        std::process::exit(1);
    }
}
