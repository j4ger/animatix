use crate::state::{SessionState, default_file_path};
use crate::text_input::{TextInput, TextInputEvent};
use gpui::{actions, div, img, App, Bounds, Context, Entity, FocusHandle, Focusable, IntoElement, KeyBinding, Render, Subscription, Task, Window, WindowBounds, WindowOptions, hsla, prelude::*, px, size};
use std::path::PathBuf;
use std::time::{Duration, Instant};

actions!(animatix_gui, [Quit]);

pub fn run_gui(path: Option<PathBuf>) {
    let file_path = path.unwrap_or_else(default_file_path);

    gpui::Application::new().run(move |cx: &mut App| {
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
            |window, cx| cx.new(|cx| AnimatixGui::new(file_path.clone(), window, cx)),
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
        let session = SessionState::load(file_path).unwrap_or_else(|error| {
            SessionState::from_error(default_file_path(), error)
        });

        let source_lines = split_lines(&session.source_text);
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
        self.session
            .set_source_text(self.source_lines.join("\n"));
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
                    this.session.preview.error = Some(error);
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
            self.session.preview.error = Some(error);
        }
        cx.notify();
    }

    fn on_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.session.reload_from_disk() {
            Ok(()) => {
                self.source_lines = split_lines(&self.session.source_text);
                self.select_line(self.selected_line.min(self.source_lines.len().saturating_sub(1)), window, cx);
            }
            Err(error) => self.session.preview.error = Some(error),
        }
        cx.notify();
    }

    fn on_play_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.toggle_playback();
        if self.session.is_playing {
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
                    if !this.session.is_playing {
                        keep_running = false;
                        return;
                    }

                    if let Err(error) = this.session.tick_playback(delta) {
                        this.session.preview.error = Some(error);
                        this.session.is_playing = false;
                        keep_running = false;
                    }

                    if !this.session.is_playing {
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
        self.session.is_playing = false;
        if let Err(error) = self
            .session
            .set_current_time(self.session.duration_s * fraction.clamp(0.0, 1.0))
        {
            self.session.preview.error = Some(error);
        }
        cx.notify();
    }

    fn render_source_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        div()
            .id("source-lines")
            .overflow_scroll()
            .flex_1()
            .border_1()
            .border_color(hsla(220. / 360., 0.08, 0.30, 1.0))
            .rounded(px(6.0))
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
                                hsla(215. / 360., 0.45, 0.26, 1.0)
                            } else {
                                hsla(220. / 360., 0.16, 0.12, 1.0)
                            })
                            .hover(|this| this.bg(hsla(215. / 360., 0.35, 0.22, 1.0)))
                            .text_color(hsla(210. / 360., 0.20, 0.92, 1.0))
                            .font_family("monospace")
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
        let segments = 40usize;
        let active = if self.session.duration_s <= f64::EPSILON {
            0usize
        } else {
            ((self.session.current_time_s / self.session.duration_s) * segments as f64)
                .round()
                .clamp(0.0, segments as f64 - 1.0) as usize
        };
        let view = cx.entity();

        div()
            .flex()
            .gap_1()
            .children((0..segments).map(move |ix| {
                let view = view.clone();
                let fill = if ix <= active {
                    hsla(195. / 360., 0.88, 0.56, 1.0)
                } else {
                    hsla(220. / 360., 0.10, 0.28, 1.0)
                };
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
        let body = if let Some(path) = &self.session.preview.image_path {
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

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(hsla(220. / 360., 0.18, 0.10, 1.0))
            .text_color(hsla(210. / 360., 0.20, 0.92, 1.0))
            .p_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(hsla(220. / 360., 0.10, 0.18, 1.0))
                            .child(format!("File: {}", self.session.file_path.display())),
                    )
                    .child(
                        div()
                            .px_2()
                            .py_1()
                            .rounded(px(4.0))
                            .bg(hsla(220. / 360., 0.10, 0.18, 1.0))
                            .child(if self.session.is_dirty { "Modified" } else { "Saved" }),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(4.0))
                                    .bg(hsla(220. / 360., 0.10, 0.18, 1.0))
                                    .child(self.session.preview.status.clone()),
                            ),
                    ),
            )
            .child(
                div()
                    .mt_3()
                    .flex()
                    .gap_3()
                    .h_full()
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child("Source")
                            .child(self.render_source_list(cx))
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .child(self.line_editor.clone()),
                                    )
                                    .child(
                                        button("+ Line", cx.entity(), |this, window, cx| {
                                            this.insert_line_after(window, cx);
                                        }),
                                    )
                                    .child(button("- Line", cx.entity(), |this, window, cx| {
                                        this.on_delete_line(window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .child(button("Save", cx.entity(), |this, window, cx| {
                                        this.on_save(window, cx);
                                    }))
                                    .child(button("Reload", cx.entity(), |this, window, cx| {
                                        this.on_reload(window, cx);
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child("Preview")
                            .child(self.render_preview())
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .child(button(if self.session.is_playing { "Pause" } else { "Play" }, cx.entity(), |this, window, cx| {
                                        this.on_play_pause(window, cx);
                                    }))
                                    .child(format!("t = {:.2}s / {:.2}s", self.session.current_time_s, self.session.duration_s)),
                            )
                            .child(self.render_timeline(cx))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(hsla(10. / 360., 0.75, 0.75, 1.0))
                                    .child(self.session.preview.error.clone().unwrap_or_else(|| "No errors".to_string())),
                            ),
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

fn button(
    text: &'static str,
    view: Entity<AnimatixGui>,
    handler: impl Fn(&mut AnimatixGui, &mut Window, &mut Context<AnimatixGui>) + 'static,
) -> impl IntoElement {
    div()
        .id(text)
        .px_2()
        .py_1()
        .rounded(px(4.0))
        .bg(hsla(215. / 360., 0.55, 0.32, 1.0))
        .text_color(gpui::white())
        .hover(|this| this.opacity(0.85))
        .child(text)
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| {
                handler(this, window, cx);
            });
        })
}
