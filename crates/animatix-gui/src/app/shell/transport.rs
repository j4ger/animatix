//! Playback controls stay with the preview across Timeline, Curves and Focus layouts.

use crate::app::i18n::render;
use egui::{Align, Layout, Rect, RichText, Vec2};
use egui_phosphor::regular as icons;
use eparts::widget::UiExt;

use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use crate::app::components::button::{Button, play_pause_icon};
use crate::app::design_tokens::spatial::spatial;
use crate::app::design_tokens::typography::TextRole;
use crate::app::i18n::text;
use crate::app::interaction::keyboard::{KeyboardAction, ShortcutRegistry, tooltip_with_shortcut};

pub(crate) fn transport_ui(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    shortcuts: &ShortcutRegistry,
) {
    let sp = spatial(ui);
    let width = ui.available_width();
    let full = width >= 480.0;
    let controls_width = if full { 240.0 } else { 128.0 };
    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);

    let (row, _) =
        ui.allocate_exact_size(Vec2::new(width, ui.available_height()), egui::Sense::hover());
    let scope = |rect, layout| egui::UiBuilder::new().max_rect(rect).layout(layout);
    let centered = Layout::left_to_right(Align::Center);
    let core = Rect::from_center_size(row.center(), Vec2::new(controls_width, row.height()));

    ui.scope_builder(scope(row.with_max_x(core.left()), centered), |ui| {
        ui.label(
            RichText::new(preview.playback.timecode_string()).font(TextRole::MonoDigits.font_id()),
        )
        .on_hover_text(render(
            ui,
            "gui-transport-time-tooltip",
            &[
                ("current", format!("{:.3}", preview.playback.current_time_s())),
                ("total", format!("{:.3}", preview.playback.duration_s)),
            ],
        ));
    });
    ui.scope_builder(scope(core, centered), |ui| {
        player_buttons(ui, preview, commands, shortcuts, full);
    });
    ui.scope_builder(
        scope(row.with_min_x(core.right()), Layout::right_to_left(Align::Center)),
        |ui| playback_options(ui, preview, commands, shortcuts, full),
    );
}

fn player_buttons(
    ui: &mut egui::Ui,
    preview: &PreviewPaneState,
    commands: &mut ActionQueue,
    shortcuts: &ShortcutRegistry,
    full: bool,
) {
    use KeyboardAction as Key;

    if full {
        action(
            ui,
            commands,
            shortcuts,
            icons::SKIP_BACK,
            "gui-transport-start",
            None,
            Command::ScrubTo(0.0),
        );
    }
    action(
        ui,
        commands,
        shortcuts,
        icons::CARET_LEFT,
        "gui-transport-previous-keyframe",
        Some(Key::PrevKeyframe),
        Command::PrevKeyframe,
    );
    if full {
        action(
            ui,
            commands,
            shortcuts,
            icons::CARET_DOUBLE_LEFT,
            "gui-transport-previous-frame",
            Some(Key::FrameStepBackward),
            Command::FrameStepBackward,
        );
    }

    let playing = preview.playback.is_playing;
    let verb = text(
        ui,
        if playing {
            "gui-transport-pause"
        } else {
            "gui-transport-play"
        },
    );
    let tooltip = tooltip_with_shortcut(shortcuts, &verb, &Key::TogglePlayback, ui.ctx());
    let play = Button::primary("").with_icon(play_pause_icon(playing)).with_tooltip(tooltip);
    if ui.add(play).clicked() {
        commands.push_back(Command::TogglePlayback.into());
    }

    if full {
        action(
            ui,
            commands,
            shortcuts,
            icons::CARET_DOUBLE_RIGHT,
            "gui-transport-next-frame",
            Some(Key::FrameStepForward),
            Command::FrameStepForward,
        );
    }
    action(
        ui,
        commands,
        shortcuts,
        icons::CARET_RIGHT,
        "gui-transport-next-keyframe",
        Some(Key::NextKeyframe),
        Command::NextKeyframe,
    );
    if full {
        action(
            ui,
            commands,
            shortcuts,
            icons::SKIP_FORWARD,
            "gui-transport-end",
            None,
            Command::ScrubTo(preview.playback.duration_s),
        );
    }
}

fn playback_options(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    shortcuts: &ShortcutRegistry,
    full: bool,
) {
    let theme = eparts::theme(ui);
    let speed = format!("{}×", preview.playback.playback_speed);
    let trigger = ui.add(
        Button::secondary(speed)
            .with_icon(icons::CARET_DOWN)
            .with_tooltip(text(ui, "gui-transport-settings")),
    );
    egui::Popup::menu(&trigger).show(|ui| settings_menu(ui, preview, commands, shortcuts, full));

    let loop_active =
        preview.playback.loop_start_s.is_some() && preview.playback.loop_end_s.is_some();
    if ui
        .add(
            Button::icon(icons::REPEAT)
                .active(loop_active)
                .with_tooltip(text(ui, "gui-transport-toggle-loop")),
        )
        .clicked()
    {
        preview.playback.loop_start_s = (!loop_active).then_some(0.0);
        preview.playback.loop_end_s = (!loop_active).then_some(preview.playback.duration_s);
    }

    if ui.available_width() > 65.0 {
        ui.label(
            RichText::new(crate::app::i18n::render(
                ui,
                "gui-transport-frames-per-second",
                &[("rate", format!("{:.0}", preview.playback.fps))],
            ))
            .font(TextRole::Mono.font_id())
            .color(theme.palette.text.secondary),
        );
    }
}

fn action(
    ui: &mut egui::Ui,
    commands: &mut ActionQueue,
    shortcuts: &ShortcutRegistry,
    icon: &'static str,
    message: &'static str,
    key: Option<KeyboardAction>,
    command: Command,
) {
    let label = text(ui, message);
    let tooltip = key
        .as_ref()
        .map(|key| tooltip_with_shortcut(shortcuts, &label, key, ui.ctx()))
        .unwrap_or(label);
    if ui.add(Button::icon(icon).with_tooltip(tooltip)).clicked() {
        commands.push_back(command.into());
    }
}

fn settings_menu(
    ui: &mut egui::Ui,
    preview: &mut PreviewPaneState,
    commands: &mut ActionQueue,
    shortcuts: &ShortcutRegistry,
    full: bool,
) {
    ui.label(RichText::new(text(ui, "gui-transport-speed")).font(TextRole::BodyMedium.font_id()));
    for (speed, label) in [(0.5_f32, "0.5×"), (1.0, "1×"), (2.0, "2×"), (4.0, "4×")] {
        let selected = (speed - preview.playback.playback_speed).abs() < f32::EPSILON;
        if ui.stable_selectable_label(selected, label).clicked() {
            preview.playback.playback_speed = speed;
            ui.close();
        }
    }

    ui.separator();
    let mut ping_pong = preview.playback.ping_pong;
    if ui
        .add(eparts::Checkbox::new(&mut ping_pong).label(&text(ui, "gui-transport-ping-pong")))
        .clicked()
    {
        preview.playback.ping_pong = ping_pong;
        if !ping_pong {
            preview.playback.ping_pong_direction = 1;
        }
    }
    let presentation_tip = tooltip_with_shortcut(
        shortcuts,
        &text(ui, "gui-presentation-mode"),
        &KeyboardAction::TogglePresentationMode,
        ui.ctx(),
    );
    if ui
        .button(text(ui, "gui-presentation-mode"))
        .on_hover_text(presentation_tip)
        .clicked()
    {
        commands.push_back(Command::TogglePresentationMode.into());
        ui.close();
    }

    if !full {
        ui.separator();
        for (label, command) in [
            ("gui-transport-start", Command::ScrubTo(0.0)),
            ("gui-transport-previous-frame", Command::FrameStepBackward),
            ("gui-transport-next-frame", Command::FrameStepForward),
            ("gui-transport-end", Command::ScrubTo(preview.playback.duration_s)),
        ] {
            if ui.button(text(ui, label)).clicked() {
                commands.push_back(command.into());
                ui.close();
            }
        }
    }
}
