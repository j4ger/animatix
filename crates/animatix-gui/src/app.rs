use crate::state::{SessionState, default_file_path};
use crate::text_input::{TextInput, TextInputEvent};
use gpui::{
    actions, div, img, App, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement,
    KeyBinding, Render, Subscription, Task, Window, WindowBounds, WindowOptions, prelude::*, px,
    size,
};
use gpui_component::{
    ActiveTheme, Root,
    button::{Button, ButtonVariants},
    h_flex,
    resizable::{h_resizable, resizable_panel},
    StyledExt,
    v_flex,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

actions!(animatix_gui, [Quit]);

pub fn run_gui(path: Option<PathBuf>) {
    let file_path = path.unwrap_or_else(default_file_path);

    gpui::Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        cx.activate(true);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("backspace", crate::text_input::Backspace, None),
            KeyBinding::new("delete", crate::text_input::Delete, None),
            KeyBinding::new("left", crate::text_input::Left, None),
            KeyBinding::new("right", crate::text_input::Right, None),
            KeyBinding::new("shift-left", crate::text_input::SelectLeft, None),
            KeyBinding::new("shift-right", crate::text_input::SelectRight, None),
            KeyBinding::new("cmd-a", crate::text_input::SelectAll, None),
            KeyBinding::new("cmd-v", crate::text_input::Paste, None),
            KeyBinding::new("cmd-c", crate::text_input::Copy, None),
            KeyBinding::new("cmd-x", crate::text_input::Cut, None),
            KeyBinding::new("home", crate::text_input::Home, None),
            KeyBinding::new("end", crate::text_input::End, None),
        ]);

        let file_path = file_path.clone();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1440.0), px(960.0)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| AnimatixGui::new(file_path.clone(), window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("Failed to open Animatix GUI window");
    });
}

pub struct AnimatixGui {
    session: SessionState,
    source_lines: Vec<String>,
    selected_line: usize,
    line_editor: Entity<TextInput>,
    focus_handle: FocusHandle,
    subscriptions: Vec<Subscription>,
    rebuild_task: Task<()>,
    playback_task: Option<Task<()>>,
    pending_rebuild_generation: u64,
}

impl AnimatixGui {
    fn new(file_path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session = SessionState::load(file_path.clone()).unwrap_or_else(|error| {
            SessionState::from_error(file_path.clone(), error)
        });

        let source_lines = split_lines(session.source_text());
        let initial_line = source_lines.first().cloned().unwrap_or_default();
        let line_editor = cx.new(|cx| TextInput::new(initial_line, "Edit selected line", cx));

        let subscriptions = vec![cx.subscribe_in(
            &line_editor,
            window,
            |this, _, event: &TextInputEvent, window, cx| {
                match event {
                    TextInputEvent::Change(text) => {
                        this.update_selected_line(text.clone());
                        this.schedule_rebuild(window, cx);
                    }
                }
            },
        )];

        Self {
            session,
            source_lines,
            selected_line: 0,
            line_editor,
            focus_handle: cx.focus_handle(),
            subscriptions,
            rebuild_task: Task::ready(()),
            playback_task: None,
            pending_rebuild_generation: 0,
        }
    }

    fn update_selected_line(&mut self, text: String) {
        if self.source_lines.is_empty() {
            self.source_lines.push(text);
            self.selected_line = 0;
        } else if let Some(line) = self.source_lines.get_mut(self.selected_line) {
            *line = text;
        }

        self.sync_source_text();
    }

    fn sync_source_text(&mut self) {
        self.session.set_source_text(self.source_lines.join("\n"));
    }

    fn schedule_rebuild(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_rebuild_generation += 1;
        let generation = self.pending_rebuild_generation;

        self.rebuild_task = cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(150))
                .await;

            let _ = view.update_in(cx, |this, _, cx| {
                if this.pending_rebuild_generation != generation {
                    return;
                }

                if let Err(error) = this.session.rebuild() {
                    this.session.preview.state.error = Some(error);
                }
                cx.notify();
            });
        });
    }

    fn select_line(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_line = index.min(self.source_lines.len().saturating_sub(1));
        let selected = self
            .source_lines
            .get(self.selected_line)
            .cloned()
            .unwrap_or_default();
        self.line_editor.update(cx, |editor, cx| {
            editor.set_value(selected, window, cx);
        });
        cx.notify();
    }

    fn insert_line_after(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let insert_at = self.selected_line.saturating_add(1).min(self.source_lines.len());
        self.source_lines.insert(insert_at, String::new());
        self.sync_source_text();
        self.select_line(insert_at, window, cx);
        self.schedule_rebuild(window, cx);
    }

    fn delete_selected_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.source_lines.len() > 1 {
            self.source_lines.remove(self.selected_line.min(self.source_lines.len() - 1));
            if self.selected_line >= self.source_lines.len() {
                self.selected_line = self.source_lines.len() - 1;
            }
        } else if let Some(line) = self.source_lines.first_mut() {
            line.clear();
        }

        self.sync_source_text();
        self.select_line(self.selected_line, window, cx);
        self.schedule_rebuild(window, cx);
    }

    fn on_delete_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.delete_selected_line(window, cx);
    }

    fn on_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.session.save_to_disk() {
            self.session.preview.state.error = Some(error);
        }
        cx.notify();
    }

    fn on_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.session.reload_from_disk() {
            Ok(()) => {
                self.source_lines = split_lines(self.session.source_text());
                self.select_line(self.selected_line.min(self.source_lines.len().saturating_sub(1)), window, cx);
            }
            Err(error) => self.session.preview.state.error = Some(error),
        }
        cx.notify();
    }

    fn on_play_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.toggle_playback();
        if self.session.preview.is_playing {
            self.start_playback_loop(window, cx);
        }
        cx.notify();
    }

    fn start_playback_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut last_tick = Instant::now();
        self.playback_task = Some(cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(120)).await;

                let now = Instant::now();
                let delta = now.saturating_duration_since(last_tick);
                last_tick = now;

                let mut keep_running = true;
                let _ = view.update_in(cx, |this, _, cx| {
                    if !this.session.preview.is_playing {
                        keep_running = false;
                        return;
                    }

                    if let Err(error) = this.session.tick_playback(delta) {
                        this.session.preview.state.error = Some(error);
                        this.session.preview.is_playing = false;
                        keep_running = false;
                    }

                    if !this.session.preview.is_playing {
                        keep_running = false;
                    }
                    cx.notify();
                });

                if !keep_running {
                    break;
                }
            }
        }));
    }

    fn set_timeline_fraction(&mut self, fraction: f64, cx: &mut Context<Self>) {
        self.session.preview.is_playing = false;
        if let Err(error) = self
            .session
            .set_current_time(self.session.preview.duration_s * fraction.clamp(0.0, 1.0))
        {
            self.session.preview.state.error = Some(error);
        }
        cx.notify();
    }

    fn render_source_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let view = cx.entity();

        div()
            .id("source-lines")
            .overflow_scroll()
            .flex_1()
            .border_1()
            .border_color(theme.border)
            .bg(theme.list)
            .rounded(theme.radius)
            .child(
                div().flex().flex_col().w_full().children(
                    self.source_lines.iter().enumerate().map(move |(index, line)| {
                        let is_selected = index == self.selected_line;
                        let view = view.clone();
                        div()
                            .id(("line", index))
                            .flex()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .bg(if is_selected {
                                theme.list_active
                            } else {
                                theme.list
                            })
                            .when(is_selected, |this| {
                                this.border_l_2().border_color(theme.list_active_border)
                            })
                            .hover(|this| this.bg(theme.list_hover))
                            .text_color(theme.foreground)
                            .font_family(theme.mono_font_family.clone())
                            .child(format!("{:>3}", index + 1))
                            .child(if line.is_empty() { " ".to_string() } else { line.clone() })
                            .on_click(move |_, window, cx| {
                                let _ = view.update(cx, |this, cx| {
                                    this.select_line(index, window, cx);
                                });
                            })
                    }),
                ),
            )
    }

    fn render_timeline(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let segments = 40usize;
        let active = if self.session.preview.duration_s <= f64::EPSILON {
            0usize
        } else {
            ((self.session.preview.current_time_s / self.session.preview.duration_s) * segments as f64)
                .round()
                .clamp(0.0, segments as f64 - 1.0) as usize
        };
        let view = cx.entity();

        div()
            .flex()
            .gap_1()
            .children((0..segments).map(move |ix| {
                let view = view.clone();
                let fill = if ix <= active { theme.primary } else { theme.muted };
                let fraction = ix as f64 / (segments.saturating_sub(1)) as f64;
                div()
                    .id(("segment", ix))
                    .flex_1()
                    .h(px(18.0))
                    .rounded(px(3.0))
                    .bg(fill)
                    .hover(|this| this.opacity(0.85))
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.set_timeline_fraction(fraction, cx);
                        });
                    })
            }))
    }

    fn render_preview(&self) -> impl IntoElement {
        let body = if let Some(path) = self
            .session
            .preview
            .state
            .artifact
            .as_ref()
            .and_then(|artifact| artifact.snapshot_path())
        {
            img(path.clone())
                .size_full()
                .object_fit(gpui::ObjectFit::Contain)
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(gpui::white())
                .child("No preview available")
                .into_any_element()
        };

        div()
            .flex_1()
            .min_h(px(320.0))
            .rounded(px(8.0))
            .bg(gpui::black())
            .overflow_hidden()
            .child(body)
    }
}

impl Focusable for AnimatixGui {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AnimatixGui {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _subscriptions_keepalive = &self.subscriptions;
        let theme = cx.theme().clone();

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                v_flex()
                    .size_full()
                    .child(
                        h_flex()
                            .px_3()
                            .py_2()
                            .gap_2()
                            .items_center()
                            .border_b_1()
                            .border_color(theme.title_bar_border)
                            .bg(theme.title_bar)
                            .child(
                                status_chip(
                                    "File",
                                    self.session.file_path().display().to_string(),
                                    theme.secondary,
                                    theme.secondary_foreground,
                                    theme.radius,
                                ),
                            )
                            .child(
                                status_chip(
                                    "State",
                                    if self.session.is_dirty() {
                                        "Modified".to_string()
                                    } else {
                                        "Saved".to_string()
                                    },
                                    if self.session.is_dirty() {
                                        theme.warning
                                    } else {
                                        theme.success
                                    },
                                    if self.session.is_dirty() {
                                        theme.warning_foreground
                                    } else {
                                        theme.success_foreground
                                    },
                                    theme.radius,
                                ),
                            )
                            .child(
                                status_chip(
                                    "Preview",
                                    self.session.preview.state.status.clone(),
                                    theme.accent,
                                    theme.accent_foreground,
                                    theme.radius,
                                ),
                            )
                            .child(div().ml_auto())
                            .child(
                                action_button("Save", "save", cx.entity(), |this, window, cx| {
                                    this.on_save(window, cx);
                                })
                                .primary(),
                            )
                            .child(
                                action_button("Reload", "reload", cx.entity(), |this, window, cx| {
                                    this.on_reload(window, cx);
                                })
                                .ghost(),
                            )
                    )
                    .child(
                        h_resizable("animatix-shell")
                            .child(
                                resizable_panel().size(px(520.0)).child(
                                    panel_shell("Source", &theme).child(
                                        v_flex()
                                            .size_full()
                                            .gap_3()
                                            .child(self.render_source_list(cx))
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(div().flex_1().child(self.line_editor.clone()))
                                                    .child(
                                                        action_button(
                                                            "+ Line",
                                                            "insert-line",
                                                            cx.entity(),
                                                            |this, window, cx| {
                                                                this.insert_line_after(window, cx);
                                                            },
                                                        )
                                                        .ghost(),
                                                    )
                                                    .child(action_button(
                                                        "- Line",
                                                        "delete-line",
                                                        cx.entity(),
                                                        |this, window, cx| {
                                                            this.on_delete_line(window, cx);
                                                        },
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(theme.muted_foreground)
                                                    .child("Line-oriented editor stays custom for precise text behavior."),
                                            ),
                                    ),
                                ),
                            )
                            .child(
                                resizable_panel().child(
                                    panel_shell("Preview", &theme).child(
                                        v_flex()
                                            .size_full()
                                            .gap_3()
                                            .child(self.render_preview())
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(action_button(
                                                        if self.session.preview.is_playing {
                                                            "Pause"
                                                        } else {
                                                            "Play"
                                                        },
                                                        "toggle-playback",
                                                        cx.entity(),
                                                        |this, window, cx| {
                                        this.on_play_pause(window, cx);
                                                        },
                                                    )
                                                    .primary())
                                                    .child(
                                                        div().text_sm().text_color(theme.muted_foreground).child(
                                                            format!(
                                                                "t = {:.2}s / {:.2}s",
                                                                self.session.preview.current_time_s,
                                                                self.session.preview.duration_s
                                                            ),
                                                        ),
                                                    ),
                                            )
                                            .child(self.render_timeline(cx))
                                            .child(
                                                div()
                                                    .rounded(theme.radius)
                                                    .border_1()
                                                    .border_color(if self.session.preview.state.error.is_some() {
                                                        theme.danger
                                                    } else {
                                                        theme.border
                                                    })
                                                    .bg(if self.session.preview.state.error.is_some() {
                                                        theme.danger
                                                    } else {
                                                        theme.secondary
                                                    })
                                                    .px_2()
                                                    .py_2()
                                                    .text_sm()
                                                    .text_color(if self.session.preview.state.error.is_some() {
                                                        theme.danger_foreground
                                                    } else {
                                                        theme.secondary_foreground
                                                    })
                                                    .child(
                                                        self.session
                                                            .preview
                                                            .state
                                                            .error
                                                            .clone()
                                                            .unwrap_or_else(|| {
                                                                "Snapshot preview backend active; future surface seam remains preserved."
                                                                    .to_string()
                                                            }),
                                                    ),
                                            ),
                                    ),
                                ),
                            )
                    ),
            )
    }
}

fn split_lines(source: &str) -> Vec<String> {
    let mut lines = source.lines().map(|line| line.to_string()).collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn panel_shell(title: &'static str, theme: &gpui_component::Theme) -> gpui::Div {
    v_flex()
        .size_full()
        .m_3()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .overflow_hidden()
        .child(
            h_flex()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.title_bar)
                .child(div().font_bold().child(title)),
        )
}

fn status_chip(
    title: &'static str,
    value: String,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
    radius: gpui::Pixels,
) -> gpui::Div {
    h_flex()
        .gap_1()
        .px_2()
        .py_1()
        .rounded(radius)
        .bg(background)
        .text_color(foreground)
        .child(div().text_xs().font_bold().child(format!("{title}:")))
        .child(div().text_sm().child(value))
}

fn action_button(
    text: &'static str,
    id: &'static str,
    view: Entity<AnimatixGui>,
    handler: impl Fn(&mut AnimatixGui, &mut Window, &mut Context<AnimatixGui>) + 'static,
) -> Button {
    Button::new(id).label(text).on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| {
                handler(this, window, cx);
            });
        })
}
