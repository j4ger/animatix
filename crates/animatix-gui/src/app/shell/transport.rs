//! Global playback transport.
//!
//! Playback is global state, so the transport lives in the top toolbar rather
//! than inside the timeline panel: switching the bottom band to another tab
//! (or collapsing it) must not take play/pause away. Timeline-local controls
//! (zoom) stay in the timeline panel.

use egui::Vec2;
use eparts::widget::UiExt;

use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, PlaybackCommand};
use crate::app::components::button::{self, Button, toolbar_separator};
use crate::app::design_tokens::typography::TextRole;

/// Render the transport controls into an existing horizontal layout.
pub(crate) fn transport_ui(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
) {
    let theme = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);

    // Go to start
    if ui
        .add(
            Button::ghost("")
                .with_icon(egui_phosphor::regular::SKIP_BACK)
                .with_tooltip("Go to start"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::ScrubTo(0.0).into());
    }

    // Previous keyframe
    if ui
        .add(
            Button::ghost("")
                .with_icon(egui_phosphor::regular::CARET_LEFT)
                .with_tooltip("Previous keyframe (,)"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::PrevKeyframe.into());
    }

    // Play / Pause
    if ui
        .add(
            Button::icon(button::play_pause_icon(preview.playback.is_playing))
                .with_tooltip("Play/Pause (Space)"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::TogglePlayback.into());
    }

    // Next keyframe
    if ui
        .add(
            Button::ghost("")
                .with_icon(egui_phosphor::regular::CARET_RIGHT)
                .with_tooltip("Next keyframe (.)"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::NextKeyframe.into());
    }

    // Frame step back / forward
    if ui
        .add(
            Button::ghost("")
                .with_icon("\u{23EA}")
                .with_tooltip("Step back one frame (Shift+,)"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::FrameStepBackward.into());
    }
    if ui
        .add(
            Button::ghost("")
                .with_icon("\u{23E9}")
                .with_tooltip("Step forward one frame (Shift+.)"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::FrameStepForward.into());
    }

    // Go to end
    if ui
        .add(
            Button::ghost("")
                .with_icon(egui_phosphor::regular::SKIP_FORWARD)
                .with_tooltip("Go to end"),
        )
        .clicked()
    {
        commands.push_back(PlaybackCommand::ScrubTo(preview.playback.duration_s).into());
    }

    toolbar_separator(ui);

    // Speed
    const SPEEDS: [(f32, &str); 4] = [
        (0.5, "\u{BD}\u{D7}"),
        (1.0, "1\u{D7}"),
        (2.0, "2\u{D7}"),
        (4.0, "4\u{D7}"),
    ];
    let si = SPEEDS
        .iter()
        .position(|(v, _)| (*v - preview.playback.playback_speed).abs() < f32::EPSILON)
        .unwrap_or(1);
    ui.menu_button(
        egui::RichText::new(SPEEDS[si].1)
            .monospace()
            .size(TextRole::BodyS.size())
            .color(theme.palette.text.secondary),
        |ui| {
            for (speed, label) in &SPEEDS {
                let is_active = (*speed - preview.playback.playback_speed).abs() < f32::EPSILON;
                if ui.stable_selectable_label(is_active, *label).clicked() {
                    preview.playback.playback_speed = *speed;
                    ui.close();
                }
            }
        },
    );

    // Loop
    let loop_active =
        preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();
    if ui
        .add(
            Button::ghost("")
                .with_icon(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                .with_tooltip("Toggle loop playback")
                .active(loop_active),
        )
        .clicked()
    {
        if loop_active {
            preview.playback.loop_start_s = None;
            preview.playback.loop_end_s = None;
        } else {
            preview.playback.loop_start_s = Some(0.0);
            preview.playback.loop_end_s = Some(preview.playback.duration_s);
        }
    }

    // Ping-pong
    let ping_pong_active = preview.playback.ping_pong;
    if ui
        .add(
            Button::ghost("")
                .with_icon(egui_phosphor::regular::ARROWS_CLOCKWISE)
                .with_tooltip("Toggle ping-pong playback (bounce at boundaries)")
                .active(ping_pong_active),
        )
        .clicked()
    {
        preview.playback.ping_pong = !preview.playback.ping_pong;
        if !preview.playback.ping_pong {
            preview.playback.ping_pong_direction = 1;
        }
    }

    // Timecode + duration + fps
    let current_tc = preview.playback.timecode_string();
    let fps_val = preview.playback.fps;
    ui.add(
        egui::Label::new(
            egui::RichText::new(format!("{current_tc}  {fps_val:.0}fps"))
                .font(TextRole::Mono.font_id())
                .color(theme.palette.text.primary),
        )
        .selectable(false),
    );
}
