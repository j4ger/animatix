//! Widget Screenshot Binary
//!
//! Renders isolated GUI components and saves them as PNG images.
//!
//! Usage:
//!   cargo run --bin widget-screenshot -- --widget property-row-float --output /tmp/out.png
//!   cargo run --bin widget-screenshot -- --list
//!   cargo run --bin widget-screenshot -- --widget card --width 400 --height 200

use std::path::PathBuf;

use animatix_gui::app::theme::*;
use animatix_gui::dev::screenshot_harness::{WIDGET_REGISTRY, render_widget};
use eframe::egui;

struct Args {
    widget: String,
    output: PathBuf,
    width: u32,
    height: u32,
    list: bool,
}

fn parse_args() -> Args {
    let mut args = std::env::args().skip(1);
    let mut widget = String::new();
    let mut output = PathBuf::from("/tmp/animatix_screenshots/widget.png");
    let mut width = 480u32;
    let mut height = 120u32;
    let mut list = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--widget" | "-w" => widget = args.next().unwrap_or_default(),
            "--output" | "-o" => output = PathBuf::from(args.next().unwrap_or_default()),
            "--width" => width = args.next().and_then(|s| s.parse().ok()).unwrap_or(480),
            "--height" => height = args.next().and_then(|s| s.parse().ok()).unwrap_or(120),
            "--list" | "-l" => list = true,
            _ => {}
        }
    }

    Args {
        widget,
        output,
        width,
        height,
        list,
    }
}

struct ScreenshotApp {
    widget: String,
    output: PathBuf,
    frame_count: u32,
    done: bool,
}

impl ScreenshotApp {
    fn new(widget: String, output: PathBuf) -> Self {
        Self {
            widget,
            output,
            frame_count: 0,
            done: false,
        }
    }
}

impl eframe::App for ScreenshotApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = BG_BASE.to_normalized_gamma_f32();
        [r, g, b, a]
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.frame_count += 1;

        // Install theme on first frame
        if self.frame_count == 1 {
            let ctx = ui.ctx().clone();
            install_theme(&ctx);

            // Register Phosphor icon font
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
            ctx.set_fonts(fonts);
        }

        // Render the widget centered with padding
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG_BASE))
            .show_inside(ui, |ui| {
                let padding = 20.0;
                let content_rect = ui.available_rect_before_wrap().shrink(padding);

                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(content_rect),
                    |ui| {
                        render_widget(ui, &self.widget);
                    },
                );
            });

        // Request screenshot on frame 2 (after theme is installed and layout is stable)
        if self.frame_count == 2 {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Screenshot(
                egui::UserData::default(),
            ));
        }

        // Handle screenshot event and exit
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

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    style.spacing.item_spacing = egui::vec2(4.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.spacing.indent = 14.0;
    style.visuals.panel_fill = BG_PANEL;
    style.visuals.window_fill = BG_PANEL;
    style.visuals.extreme_bg_color = BG_BASE;
    style.visuals.faint_bg_color = BG_SURFACE;
    style.visuals.widgets.noninteractive.bg_fill = BG_SURFACE;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_SECONDARY);
    style.visuals.widgets.inactive.bg_fill = BG_WIDGET;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT_PRIMARY);
    ctx.set_global_style(style);
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
        eprintln!("Usage: widget-screenshot --widget <name> [--output <path>] [--width <px>] [--height <px>]");
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
            let app = ScreenshotApp::new(args.widget.clone(), args.output.clone());
            Ok(Box::new(app))
        }),
    ) {
        eprintln!("Failed to run widget screenshot: {e}");
        std::process::exit(1);
    }
}
