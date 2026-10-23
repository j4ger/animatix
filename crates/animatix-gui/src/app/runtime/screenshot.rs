/// Bounded whole-window screenshot driver (visual layout review).
///
/// Enabled with `ANIMATIX_SCREENSHOT=<png>` (and optional
/// `ANIMATIX_SCREENSHOT_FRAMES=<n>`), only compiled with `dev-screenshots`.
///
/// `ANIMATIX_SCREENSHOT_SIZE=<w>x<h>` sets the window size before the capture,
/// so a review can cover both the wide layout and the compact one (mirrors the
/// `--width`/`--height` flags of the `widget-screenshot` binary).
#[cfg(feature = "dev-screenshots")]
struct ScreenshotSession {
    path: PathBuf,
    after_frames: u32,
    frame: u32,
    requested: bool,
    size: Option<egui::Vec2>,
}

#[cfg(feature = "dev-screenshots")]
impl ScreenshotSession {
    fn from_env() -> Option<Self> {
        let path = PathBuf::from(std::env::var_os("ANIMATIX_SCREENSHOT")?);
        let after_frames = std::env::var("ANIMATIX_SCREENSHOT_FRAMES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8);
        let size = std::env::var("ANIMATIX_SCREENSHOT_SIZE").ok().and_then(|value| {
            let (w, h) = value.split_once('x')?;
            Some(egui::Vec2::new(w.trim().parse().ok()?, h.trim().parse().ok()?))
        });
        Some(Self {
            path,
            after_frames,
            frame: 0,
            requested: false,
            size,
        })
    }
}

#[cfg(feature = "dev-screenshots")]
impl AnimatixApp {
    fn drive_screenshot(&mut self, ui: &mut egui::Ui) {
        let Some(session) = self.screenshot.as_mut() else {
            return;
        };
        session.frame += 1;
        if session.frame < session.after_frames {
            if session.frame == 1 {
                if let Some(size) = session.size {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                }
            }
            ui.ctx().request_repaint();
            return;
        }
        if !session.requested {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            session.requested = true;
        }

        let mut saved = false;
        ui.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    match save_window_screenshot(image, i.pixels_per_point, &session.path) {
                        Ok(()) => {
                            tracing::info!(
                                "workspace screenshot saved to {}",
                                session.path.display()
                            );
                            saved = true;
                        },
                        Err(error) => tracing::error!("workspace screenshot failed: {error}"),
                    }
                }
            }
        });

        if saved {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ui.ctx().request_repaint();
        }
    }
}

#[cfg(feature = "dev-screenshots")]
fn save_window_screenshot(
    image: &egui::ColorImage,
    pixels_per_point: f32,
    path: &std::path::Path,
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
