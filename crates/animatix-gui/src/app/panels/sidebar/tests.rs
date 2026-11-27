use super::*;
use crate::app::PreviewPaneState;
use crate::app::document::{DocumentSession, review_scene_at_time};
use crate::app::handlers::scene::handle_select_scene;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};
use crate::editor::EditorBuffer;
use egui::{Context, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, vec2};

struct Navigator {
    egui: Context,
    tab: SidebarTab,
    files: Vec<FileTreeEntry>,
    file_tree_revision: u64,
    expanded_dirs: HashSet<PathBuf>,
    commands: ActionQueue,
    selected: HashSet<String>,
    collapsed: HashSet<String>,
}

impl Navigator {
    fn new(tab: SidebarTab) -> Self {
        let egui = Context::default();
        crate::fonts::install_fonts(&egui);
        Self {
            egui,
            tab,
            files: vec![],
            file_tree_revision: 0,
            expanded_dirs: HashSet::new(),
            commands: ActionQueue::new(),
            selected: HashSet::new(),
            collapsed: HashSet::new(),
        }
    }

    fn frame(
        &mut self,
        document: &DocumentSession,
        events: Vec<Event>,
    ) -> Vec<egui::epaint::TextShape> {
        let mut output = self.egui.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(170.0, 380.0))),
                events,
                ..Default::default()
            },
            |ui| {
                sidebar_ui(
                    &mut SidebarContext {
                        document,
                        expanded_dirs: &self.expanded_dirs,
                        file_tree: &self.files,
                        file_tree_revision: self.file_tree_revision,
                        file_tree_loading: false,
                        commands: &mut self.commands,
                        selected_actors: &mut self.selected,
                        collapsed_actors: &mut self.collapsed,
                        sidebar_tab: &mut self.tab,
                        compact: false,
                        compact_drawer: &mut None,
                    },
                    ui,
                );
            },
        );
        output.textures_delta.clear();
        let mut text = vec![];
        for shape in output.shapes {
            extract_text(shape.shape, &mut text);
        }
        text
    }
}

fn extract_text(shape: Shape, text: &mut Vec<egui::epaint::TextShape>) {
    match shape {
        Shape::Text(label) => text.push(label),
        Shape::Vec(shapes) => {
            for nested in shapes {
                extract_text(nested, text);
            }
        },
        _ => {}, // Paint and decorative geometry are not the label hit targets.
    }
}

fn label_center(shapes: &[egui::epaint::TextShape], name: &str) -> Pos2 {
    let label = shapes.iter().find(|s| s.galley.job.text == name).unwrap_or_else(|| {
        let labels: Vec<_> = shapes.iter().map(|s| &s.galley.job.text).collect();
        panic!("{name:?} missing from {labels:?}");
    });
    label.pos + label.galley.size() * 0.5
}

fn press(pos: Pos2, pressed: bool) -> Vec<Event> {
    vec![
        Event::PointerMoved(pos),
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        },
    ]
}

#[test]
fn project_keeps_files_clickable_when_resources_are_disclosed() {
    let document = composition_document();
    let mut nav = Navigator::new(SidebarTab::Project);
    nav.files.push(FileTreeEntry {
        name: "demo.amx".to_owned(),
        path: "demo.amx".into(),
        depth: 0,
        is_dir: false,
    });
    let first = nav.frame(&document, vec![]);
    let file = label_center(&first, "demo.amx");
    let disclosure = label_center(&first, "Resources");
    assert!(file.y < disclosure.y);
    assert!(!first.iter().any(|s| s.galley.job.text == "Components"));

    for down in [true, false] {
        nav.frame(&document, press(disclosure, down));
    }
    nav.egui.all_styles_mut(|style| style.animation_time = 0.0);
    let opened = nav.frame(&document, vec![]);
    assert!(label_center(&opened, "Referenced assets").y > disclosure.y);
    assert!(label_center(&opened, "Components").y > disclosure.y);
    assert_eq!(opened.iter().filter(|s| s.galley.job.text == "mark.svg").count(), 1);
    assert!(label_center(&opened, "second.svg").y > disclosure.y);
    for down in [true, false] {
        nav.frame(&document, press(file, down));
    }
    assert!(
        matches!(nav.commands.pop_front(), Some(ShellAction::Command(Command::OpenFile(path))) if path == Path::new("demo.amx"))
    );
}

#[test]
fn explorer_refreshes_cached_filtered_rows_after_a_snapshot_change() {
    let document = composition_document();
    let mut nav = Navigator::new(SidebarTab::Project);
    nav.egui.data_mut(|data| {
        data.insert_temp(egui::Id::unique(EXPLORER_FILTER_ID), "match".to_owned());
    });

    for (revision, name) in [(1, "match-old.amx"), (2, "match-new.amx")] {
        nav.files = vec![
            FileTreeEntry {
                name: "scenes".to_owned(),
                path: "scenes".into(),
                depth: 0,
                is_dir: true,
            },
            FileTreeEntry {
                name: name.to_owned(),
                path: Path::new("scenes").join(name),
                depth: 1,
                is_dir: false,
            },
        ];
        nav.file_tree_revision = revision;
        let labels = nav.frame(&document, vec![]);
        assert!(label_center(&labels, "scenes").y < label_center(&labels, name).y);
    }
}

#[test]
fn outline_scene_choice_keeps_layers_in_place_and_hides_in_single_scene() {
    let document = composition_document();
    assert!(document.timeline.is_none(), "composition must not have a raw timeline");
    let editor = EditorBuffer::new(&document.file_path, document.source_text.clone());
    let mut preview =
        PreviewStore::new(PreviewPaneState::new(document.duration_s, document.scene_dimensions));
    let mut store = DocumentStore::new(document, editor);
    let mut nav = Navigator::new(SidebarTab::Outline);
    // Even without a stored active-scene name, chooser and layers use the same fallback.
    store.source.document.active_scene = None;
    let first = nav.frame(&store.source.document, vec![]);
    let picker = label_center(&first, "Intro");
    let layer = label_center(&first, "title_rect");
    assert!(picker.y < layer.y);
    assert!(!first.iter().any(|s| s.galley.job.text == "graph_rect"));
    for down in [true, false] {
        nav.frame(&store.source.document, press(picker, down));
    }
    let popup = nav.frame(&store.source.document, vec![]);
    let diagram = label_center(&popup, "Diagram");
    for down in [true, false] {
        nav.frame(&store.source.document, press(diagram, down));
    }
    let Some(ShellAction::Command(Command::SelectScene(name))) = nav.commands.pop_front() else {
        panic!("clicking the scene chooser did not select Diagram");
    };
    assert_eq!(name, "Diagram");
    handle_select_scene(
        &mut store,
        &mut preview,
        &mut UiStore::new(egui_tiles::Tree::empty(egui::Id::unique("navigator_test"))),
        name,
    );
    // The shell recomputes the active scene from the playhead on the following frame.
    let composition = store.source.document.composition.as_ref().unwrap();
    assert_eq!(review_scene_at_time(composition, 2.999).as_deref(), Some("Intro"));
    assert_eq!(review_scene_at_time(composition, 3.0_f64.next_down()).as_deref(), Some("Intro"));
    assert_eq!(preview.preview.playback.current_time_s(), 3.0);
    let selected = review_scene_at_time(composition, preview.preview.playback.current_time_s());
    assert_eq!(
        selected.as_deref(),
        Some("Diagram"),
        "fade completion must not revert the picker"
    );
    store.source.document.active_scene = selected;
    let second = nav.frame(&store.source.document, vec![]);
    assert!(label_center(&second, "Diagram").y < label_center(&second, "graph_rect").y);
    assert!(!second.iter().any(|s| s.galley.job.text == "title_rect"));

    let graph_label = label_center(&second, "graph_rect");
    for down in [true, false] {
        nav.frame(&store.source.document, press(graph_label, down));
    }
    assert!(nav.selected.contains("graph_rect"));
    let mut single =
        DocumentSession::from_source("single.amx".into(), "#0s\ngraph_rect: Rect".into()).unwrap();
    single.rebuild().unwrap();
    let single = nav.frame(&single, vec![]);
    assert!(!single.iter().any(|s| s.galley.job.text == "Diagram"));
    assert!(label_center(&single, "graph_rect").y < graph_label.y);
}

fn composition_document() -> DocumentSession {
    let directory = tempfile::tempdir().unwrap();
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#;
    for file in ["mark.svg", "second.svg"] {
        std::fs::write(directory.path().join(file), svg).unwrap();
    }
    let path = directory.path().join("main.amx");
    std::fs::write(
        &path,
        r#"# Intro
config { duration: 3 }
#0s
title_rect: Rect, size: (20, 20)
intro_mark: Svg, url: "mark.svg"
play Diagram [fade, 300ms]

# Diagram
config { duration: 4 }
#0s
graph_rect: Rect, size: (40, 40)
diagram_mark: Svg, url: "mark.svg"
unique_mark: Svg, url: "second.svg"
"#,
    )
    .unwrap();
    DocumentSession::load(path).unwrap()
}
