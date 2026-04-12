use crate::state::{SessionState, default_file_path};
use gpui::{
    actions, div, img, prelude::*, px, size, App, Bounds, Context, Entity, FocusHandle,
    Focusable, IntoElement, KeyBinding, Render, Subscription, Task, Window, WindowBounds,
    WindowOptions,
};
use gpui_component::{
    input::{Input, InputEvent, InputState, TabSize},
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    theme::{Theme, ThemeMode},
    ActiveTheme, Root,
    button::{Button, ButtonVariants},
    h_flex,
    StyledExt,
    v_flex,
};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

actions!(animatix_gui, [Quit]);

const MAX_FILE_TREE_DEPTH: usize = 3;
const MAX_FILE_TREE_ENTRIES: usize = 160;

pub fn run_gui(path: Option<PathBuf>) {
    let file_path = path.unwrap_or_else(default_file_path);

    gpui::Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);
        cx.activate(true);
        cx.on_action(|_: &Quit, cx: &mut App| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

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

#[derive(Clone)]
struct FileTreeEntry {
    path: PathBuf,
    depth: usize,
    is_dir: bool,
    is_current: bool,
}

pub struct AnimatixGui {
    session: SessionState,
    editor: Entity<InputState>,
    workspace_root: PathBuf,
    file_tree: Vec<FileTreeEntry>,
    applying_editor_value: bool,
    focus_handle: FocusHandle,
    subscriptions: Vec<Subscription>,
    rebuild_task: Task<()>,
    playback_task: Option<Task<()>>,
    pending_rebuild_generation: u64,
}

impl AnimatixGui {
    fn new(file_path: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let session = SessionState::load(file_path.clone())
            .unwrap_or_else(|error| SessionState::from_error(file_path.clone(), error));
        let workspace_root = workspace_root_for(session.file_path());
        let file_tree = build_file_tree(&workspace_root, session.file_path());
        let language = editor_language(session.file_path());
        let initial_source = session.source_text().to_owned();

        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor(language)
                .line_number(true)
                .soft_wrap(false)
                .tab_size(TabSize {
                    tab_size: 2,
                    hard_tabs: false,
                })
                .default_value(initial_source)
        });

        let subscriptions = vec![cx.subscribe_in(
            &editor,
            window,
            |this, state, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    let next_source = state.read(cx).value().to_string();
                    if this.applying_editor_value || next_source == this.session.source_text() {
                        return;
                    }
                    this.session.set_source_text(next_source);
                    this.schedule_rebuild(window, cx);
                }
            },
        )];

        Self {
            session,
            editor,
            workspace_root,
            file_tree,
            applying_editor_value: false,
            focus_handle: cx.focus_handle(),
            subscriptions,
            rebuild_task: Task::ready(()),
            playback_task: None,
            pending_rebuild_generation: 0,
        }
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

                let previous_artifact = this.session.preview.state.artifact.clone();

                if let Err(error) = this.session.rebuild() {
                    this.session.preview.state.error = Some(error);
                }
                this.drop_replaced_preview_image(previous_artifact, None, cx);
                cx.notify();
            });
        });
    }

    fn on_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.session.save_to_disk() {
            self.session.preview.state.error = Some(error);
        }
        cx.notify();
    }

    fn on_reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_rebuild_generation += 1;
        let previous_artifact = self.session.preview.state.artifact.clone();
        match self.session.reload_from_disk() {
            Ok(()) => {
                let source = self.session.source_text().to_owned();
                self.applying_editor_value = true;
                self.editor
                    .update(cx, |editor, cx| editor.set_value(source, window, cx));
                self.applying_editor_value = false;
                self.file_tree = build_file_tree(&self.workspace_root, self.session.file_path());
            }
            Err(error) => self.session.preview.state.error = Some(error),
        }
        self.drop_replaced_preview_image(previous_artifact, Some(window), cx);
        cx.notify();
    }

    fn on_play_pause(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.toggle_playback();
        if self.session.preview.is_playing {
            self.start_playback_loop(window, cx);
        }
        cx.notify();
    }

    fn on_rebuild(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let previous_artifact = self.session.preview.state.artifact.clone();
        if let Err(error) = self.session.rebuild() {
            self.session.preview.state.error = Some(error);
        }
        self.drop_replaced_preview_image(previous_artifact, None, cx);
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

                    let previous_artifact = this.session.preview.state.artifact.clone();

                    if let Err(error) = this.session.tick_playback(delta) {
                        this.session.preview.state.error = Some(error);
                        this.session.preview.is_playing = false;
                        keep_running = false;
                    }

                    this.drop_replaced_preview_image(previous_artifact, None, cx);

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
        let previous_artifact = self.session.preview.state.artifact.clone();
        if let Err(error) = self
            .session
            .set_current_time(self.session.preview.duration_s * fraction.clamp(0.0, 1.0))
        {
            self.session.preview.state.error = Some(error);
        }
        self.drop_replaced_preview_image(previous_artifact, None, cx);
        cx.notify();
    }

    fn drop_replaced_preview_image(
        &self,
        previous_artifact: Option<crate::preview::artifact::PreviewArtifact>,
        window: Option<&mut Window>,
        cx: &mut App,
    ) {
        let Some(previous_image) = previous_artifact.and_then(|artifact| match artifact {
            crate::preview::artifact::PreviewArtifact::Image(image) => Some(image),
            crate::preview::artifact::PreviewArtifact::FutureSurface => None,
        }) else {
            return;
        };

        let current_image_id = self
            .session
            .preview
            .state
            .artifact
            .as_ref()
            .and_then(|artifact| artifact.render_image())
            .map(|image| image.id);

        if Some(previous_image.id) != current_image_id {
            cx.drop_image(previous_image, window);
        }
    }

    fn render_file_tree(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        div().flex_1().overflow_scrollbar().child(
            v_flex().w_full().children(self.file_tree.iter().map(|entry| {
                let depth_indent = px((entry.depth as f32) * 14.0);
                let icon = if entry.is_dir { "▾" } else { "•" };
                let name = entry
                    .path
                    .file_name()
                    .unwrap_or_else(|| OsStr::new("workspace"))
                    .to_string_lossy()
                    .to_string();
                let metadata = if entry.is_dir {
                    "folder".to_string()
                } else {
                    entry.path
                        .strip_prefix(&self.workspace_root)
                        .unwrap_or(&entry.path)
                        .display()
                        .to_string()
                };

                h_flex()
                    .id(("file-tree", file_tree_entry_id(entry)))
                    .w_full()
                    .gap_2()
                    .items_start()
                    .px_2()
                    .py_1()
                    .pl_2()
                    .when(entry.is_current, |this| {
                        this.bg(theme.list_active)
                            .text_color(theme.foreground)
                            .border_l_2()
                            .border_color(theme.list_active_border)
                    })
                    .when(!entry.is_current, |this| {
                        this.text_color(if entry.is_dir {
                            theme.muted_foreground
                        } else {
                            theme.foreground
                        })
                    })
                    .child(div().pt_1().text_sm().text_color(theme.muted_foreground).child(icon))
                    .child(
                        v_flex()
                            .gap_1()
                            .pl(depth_indent)
                            .child(div().text_sm().font_family(theme.mono_font_family.clone()).child(name))
                            .child(div().text_xs().text_color(theme.muted_foreground).child(metadata)),
                    )
            })),
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

        div().flex().gap_1().children((0..segments).map(move |ix| {
            let view = view.clone();
            let fill = if ix <= active { theme.primary } else { theme.muted };
            let fraction = ix as f64 / (segments.saturating_sub(1)) as f64;
            div()
                .id(("timeline-segment", ix))
                .flex_1()
                .h(px(16.0))
                .rounded(theme.radius)
                .bg(fill)
                .hover(|this| this.opacity(0.85))
                .on_click(move |_, _, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.set_timeline_fraction(fraction, cx);
                    });
                })
        }))
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let body = if let Some(image) = self
            .session
            .preview
            .state
            .artifact
            .as_ref()
            .and_then(|artifact| artifact.render_image())
        {
            img(image.clone())
                .size_full()
                .object_fit(gpui::ObjectFit::Contain)
                .into_any_element()
        } else {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .font_family(theme.mono_font_family.clone())
                .child("No preview available")
                .into_any_element()
        };

        div()
            .flex_1()
            .min_h(px(280.0))
            .rounded(theme.radius_lg)
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
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
        let file_name = self
            .session
            .file_path()
            .file_name()
            .unwrap_or_else(|| OsStr::new("untitled"))
            .to_string_lossy()
            .to_string();
        let file_path = self.session.file_path().display().to_string();
        let dirty_label = if self.session.is_dirty() { "Modified" } else { "Saved" };
        let preview_message = self
            .session
            .preview
            .state
            .error
            .clone()
            .unwrap_or_else(|| self.session.preview.state.status.clone());

        div()
            .size_full()
            .track_focus(&self.focus_handle)
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                v_flex()
                    .size_full()
                    .bg(theme.background)
                    .child(
                        h_flex()
                            .px_3()
                            .py_2()
                            .gap_3()
                            .items_center()
                            .border_b_1()
                            .border_color(theme.border)
                            .bg(theme.title_bar)
                            .child(
                                h_flex()
                                    .gap_3()
                                    .items_center()
                                    .child(div().font_bold().child("Animatix"))
                                    .child(
                                        h_flex()
                                            .gap_2()
                                            .text_sm()
                                            .text_color(theme.muted_foreground)
                                            .child("File")
                                            .child("Edit")
                                            .child("Preview")
                                            .child("Run"),
                                    ),
                            )
                            .child(div().ml_auto())
                            .child(
                                status_badge(
                                    dirty_label,
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
                            .child(action_button("Save", "save", cx.entity(), |this, window, cx| {
                                this.on_save(window, cx);
                            })
                            .primary())
                            .child(action_button(
                                "Reload",
                                "reload",
                                cx.entity(),
                                |this, window, cx| {
                                    this.on_reload(window, cx);
                                },
                            )
                            .ghost())
                            .child(action_button(
                                "Rebuild",
                                "rebuild",
                                cx.entity(),
                                |this, window, cx| {
                                    this.on_rebuild(window, cx);
                                },
                            )
                            .ghost())
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
                            .primary()),
                    )
                    .child(
                        h_resizable("animatix-editor-shell")
                            .child(
                                resizable_panel().size(px(260.0)).child(
                                    editor_panel(
                                        "Explorer",
                                        self.workspace_root.display().to_string(),
                                        &theme,
                                    )
                                    .child(self.render_file_tree(cx)),
                                ),
                            )
                            .child(
                                resizable_panel().size(px(720.0)).child(
                                    editor_panel("Editor", file_name.clone(), &theme).child(
                                        v_flex()
                                            .size_full()
                                            .gap_1()
                                            .child(
                                                h_flex()
                                                    .px_3()
                                                    .py_2()
                                                    .gap_2()
                                                    .items_center()
                                                    .border_b_1()
                                                    .border_color(theme.border)
                                                    .bg(theme.background)
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_family(theme.mono_font_family.clone())
                                                            .child(file_name),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(theme.muted_foreground)
                                                            .child(file_path.clone()),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .bg(theme.background)
                                                    .child(
                                                        Input::new(&self.editor)
                                                            .h_full()
                                                            .appearance(false)
                                                            .bordered(false)
                                                            .focus_bordered(false),
                                                    ),
                                            ),
                                    ),
                                ),
                            )
                            .child(
                                resizable_panel().size(px(420.0)).child(
                                    editor_panel(
                                        "Preview",
                                        self.session.preview.state.status.clone(),
                                        &theme,
                                    )
                                    .child(
                                        v_flex()
                                            .size_full()
                                            .gap_3()
                                            .child(self.render_preview(cx))
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .text_color(theme.muted_foreground)
                                                            .child(format!(
                                                                "t = {:.2}s / {:.2}s",
                                                                self.session.preview.current_time_s,
                                                                self.session.preview.duration_s
                                                            )),
                                                    )
                                                    .child(div().ml_auto())
                                                    .child(status_badge(
                                                        if self.session.preview.is_playing {
                                                            "Playing"
                                                        } else {
                                                            "Paused"
                                                        },
                                                        theme.accent,
                                                        theme.accent_foreground,
                                                        theme.radius,
                                                    )),
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
                                                    .px_3()
                                                    .py_2()
                                                    .text_sm()
                                                    .text_color(if self.session.preview.state.error.is_some() {
                                                        theme.danger_foreground
                                                    } else {
                                                        theme.secondary_foreground
                                                    })
                                                    .child(preview_message),
                                            ),
                                    ),
                                ),
                            ),
                    )
                    .child(
                        h_flex()
                            .px_3()
                            .py_2()
                            .gap_3()
                            .items_center()
                            .border_t_1()
                            .border_color(theme.border)
                            .bg(theme.title_bar)
                            .text_sm()
                            .child(
                                div()
                                    .font_family(theme.mono_font_family.clone())
                                    .child(file_path),
                            )
                            .child(div().text_color(theme.muted_foreground).child("•"))
                            .child(
                                div().child(if self.session.is_dirty() {
                                    "Dirty"
                                } else {
                                    "Saved"
                                }),
                            )
                            .child(div().text_color(theme.muted_foreground).child("•"))
                            .child(div().child(format!(
                                "{:.2}s / {:.2}s",
                                self.session.preview.current_time_s, self.session.preview.duration_s
                            )))
                            .child(div().text_color(theme.muted_foreground).child("•"))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(self.session.preview.state.status.clone()),
                            )
                            .when_some(self.session.preview.state.error.clone(), |this, error| {
                                this.child(div().text_color(theme.danger_foreground).child(error))
                            }),
                    ),
            )
    }
}

fn editor_panel(title: &'static str, subtitle: String, theme: &gpui_component::Theme) -> gpui::Div {
    v_flex()
        .size_full()
        .m_2()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .overflow_hidden()
        .child(
            h_flex()
                .px_3()
                .py_2()
                .gap_2()
                .items_center()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.title_bar)
                .child(div().font_bold().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .child(subtitle),
                ),
        )
}

fn file_tree_entry_id(entry: &FileTreeEntry) -> usize {
    entry
        .path
        .to_string_lossy()
        .bytes()
        .fold(entry.depth, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as usize))
}

fn status_badge(
    text: impl Into<gpui::SharedString>,
    background: gpui::Hsla,
    foreground: gpui::Hsla,
    radius: gpui::Pixels,
) -> gpui::Div {
    div()
        .px_2()
        .py_1()
        .rounded(radius)
        .bg(background)
        .text_color(foreground)
        .text_sm()
        .child(text.into())
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

fn workspace_root_for(file_path: &Path) -> PathBuf {
    let search_start = file_path.parent().unwrap_or(file_path);
    for ancestor in search_start.ancestors() {
        if ancestor.join("Cargo.toml").exists() || ancestor.join(".git").exists() {
            return ancestor.to_path_buf();
        }
    }
    search_start.to_path_buf()
}

fn build_file_tree(workspace_root: &Path, current_file: &Path) -> Vec<FileTreeEntry> {
    let mut entries = Vec::new();
    collect_tree_entries(
        workspace_root,
        workspace_root,
        current_file,
        0,
        &mut entries,
        &mut 0,
    );

    if !entries.iter().any(|entry| entry.path == current_file) {
        entries.push(FileTreeEntry {
            path: current_file.to_path_buf(),
            depth: current_file
                .strip_prefix(workspace_root)
                .ok()
                .map(|path| path.components().count().saturating_sub(1))
                .unwrap_or(0),
            is_dir: false,
            is_current: true,
        });
    }

    entries
}

fn collect_tree_entries(
    workspace_root: &Path,
    dir: &Path,
    current_file: &Path,
    depth: usize,
    entries: &mut Vec<FileTreeEntry>,
    seen: &mut usize,
) {
    if depth > MAX_FILE_TREE_DEPTH || *seen >= MAX_FILE_TREE_ENTRIES {
        return;
    }

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    let mut children = read_dir
        .flatten()
        .filter(|entry| !is_hidden(&entry.path()))
        .collect::<Vec<_>>();

    children.sort_by(|left, right| {
        let left_path = left.path();
        let right_path = right.path();
        let left_is_dir = left_path.is_dir();
        let right_is_dir = right_path.is_dir();

        right_is_dir
            .cmp(&left_is_dir)
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });

    for child in children {
        if *seen >= MAX_FILE_TREE_ENTRIES {
            break;
        }

        let path = child.path();
        let is_dir = path.is_dir();
        let is_current = path == current_file;
        let contains_current = current_file.starts_with(&path);
        let should_show = depth < 2
            || is_current
            || contains_current
            || same_parent(&path, current_file)
            || matches_relevant_file(&path);

        if !should_show {
            continue;
        }

        *seen += 1;
        entries.push(FileTreeEntry {
            path: path.clone(),
            depth,
            is_dir,
            is_current,
        });

        if is_dir && (contains_current || depth < 1) {
            collect_tree_entries(
                workspace_root,
                &path,
                current_file,
                depth + 1,
                entries,
                seen,
            );
        }
    }

    let _ = workspace_root;
}

fn same_parent(path: &Path, current_file: &Path) -> bool {
    path.parent()
        .zip(current_file.parent())
        .map(|(left, right)| left == right)
        .unwrap_or(false)
}

fn matches_relevant_file(path: &Path) -> bool {
    path.is_dir()
        || path
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| matches!(ext, "amx" | "rs" | "toml" | "md"))
            .unwrap_or(false)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn editor_language(path: &Path) -> &'static str {
    match path.extension().and_then(OsStr::to_str) {
        Some("rs") => "rust",
        Some("json") => "json",
        Some("toml") => "toml",
        Some("md") => "markdown",
        Some("html") => "html",
        Some("css") => "css",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("yaml" | "yml") => "yaml",
        _ => "rust",
    }
}
