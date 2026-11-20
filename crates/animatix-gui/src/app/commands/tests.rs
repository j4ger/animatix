use super::*;

mod command_queue_tests {
    use super::*;

    #[test]
    fn command_sender_enqueues_and_drains_commands() {
        let mut queue = CommandQueue::new();
        let sender = queue.sender();
        sender.send_command(Command::Rebuild).expect("queue should be open");
        sender.send_command(Command::ZoomToAll).expect("queue should be open");

        let mut executed = Vec::new();
        queue.drain_with(|action| executed.push(action));

        assert_eq!(executed.len(), 2);
        assert!(matches!(executed[0], ShellAction::Command(Command::Rebuild)));
        assert!(matches!(executed[1], ShellAction::Command(Command::ZoomToAll)));

        let mut remaining = Vec::new();
        queue.drain_with(|action| remaining.push(action));
        assert!(remaining.is_empty(), "drain should consume every queued command");
    }

    #[test]
    fn command_queue_drain_appends_to_per_frame_queue() {
        let mut queue = CommandQueue::new();
        queue.push(Command::Rebuild);

        let mut frame_actions = ActionQueue::default();
        queue.drain_into(&mut frame_actions);
        assert_eq!(frame_actions.len(), 1);
        assert!(matches!(frame_actions[0], ShellAction::Command(Command::Rebuild)));

        let mut remaining = Vec::new();
        queue.drain_with(|action| remaining.push(action));
        assert!(remaining.is_empty(), "drain_into should consume every queued command");
    }
}

mod workbench_boundary_tests {
    use super::*;
    use crate::app::{GuiShell, WorkspaceTab, persistence};

    fn shell() -> GuiShell {
        let mut shell =
            GuiShell::load(std::path::PathBuf::from("missing-workbench-test.amx"), false);
        shell.document_store = crate::app::test_support::make_document_store("#0s\n");
        shell
    }

    #[test]
    fn review_actions_change_view_state_without_editing_source_or_history() {
        let mut shell = shell();
        let source = r#"# Intro
#0s
intro: Text, text: "Intro"

# Outro
#0s
outro: Text, text: "Outro"
"#;
        shell.document_store = crate::app::test_support::make_document_store(source);
        let before = shell.document_store.source.document.source_text.clone();
        let actions = [
            Command::SelectScene("Outro".into()),
            Command::ScrubTo(0.5),
            Command::TogglePlayback,
        ];
        for command in actions {
            shell.handle_action(command.into());
        }
        assert_eq!(shell.document_store.source.document.source_text, before);
        assert!(shell.document_store.history.undo_stack.is_empty());
        assert!(shell.document_store.history.redo_stack.is_empty());
        assert_eq!(shell.document_store.source.document.active_scene.as_deref(), Some("Outro"));
    }

    #[test]
    fn source_editor_changes_remain_authoritative() {
        let mut shell = shell();
        shell
            .document_store
            .source
            .editor
            .set_document(std::path::Path::new("test.amx"), "#0s\n#1s\n".into());
        shell.handle_action(Command::EditorChanged.into());
        assert_eq!(shell.document_store.source.document.source_text, "#0s\n#1s\n");
        assert!(shell.preview_store.has_pending_rebuild());
    }

    #[test]
    fn narration_patch_is_one_undoable_source_edit_and_cannot_be_replayed() {
        use crate::editor::source_patch::SourcePatch;

        let mut shell = shell();
        let source = "// 原始注释\r\n#1s:\r\nbox: Rect\r\n";
        shell.document_store = crate::app::test_support::make_document_store(source);
        let start = source.find("#1s").unwrap();
        let store = &shell.document_store.source;
        let patch = SourcePatch::new(
            &store.editor,
            store.epoch(),
            start..start + 3,
            "#narration@marker".into(),
            "Bind narration timing",
        )
        .unwrap();

        shell.handle_action(Command::ApplySourcePatch(patch.clone()).into());
        let updated = source.replace("#1s", "#narration@marker");
        assert_eq!(shell.document_store.source.editor.text(), updated);
        assert_eq!(shell.document_store.source.text(), updated);
        assert_eq!(shell.document_store.history.undo_stack.len(), 1);
        assert!(shell.preview_store.has_pending_rebuild());

        shell.handle_action(Command::Undo.into());
        assert_eq!(shell.document_store.source.text(), source);
        shell.handle_action(Command::ApplySourcePatch(patch).into());
        assert_eq!(shell.document_store.source.text(), source);
        assert!(shell.document_store.history.undo_stack.is_empty());
        shell.handle_action(Command::Redo.into());
        assert_eq!(shell.document_store.source.text(), updated);
    }

    #[test]
    fn source_navigation_reveals_code_without_toggling_it_closed() {
        let mut shell = shell();
        shell.ui_store.view.compact = false;
        persistence::set_active_detail_tab(&mut shell.ui_store.view.tree, WorkspaceTab::Inspector);
        persistence::set_detail_visible(&mut shell.ui_store.view.tree, false);
        for _ in 0..2 {
            shell.handle_action(Command::ScrollToLine(1, 1).into());
            assert!(persistence::detail_visible(&shell.ui_store.view.tree));
            assert_eq!(
                persistence::active_detail_tab(&shell.ui_store.view.tree),
                Some(WorkspaceTab::Code)
            );
        }
    }

    #[test]
    fn source_navigation_opens_compact_code_drawer() {
        let mut shell = shell();
        shell.ui_store.view.compact = true;
        shell.ui_store.view.compact_drawer = None;
        shell.handle_action(Command::ScrollToLine(1, 1).into());
        assert_eq!(
            shell.ui_store.view.compact_drawer,
            Some(crate::app::panels::CompactDrawer::Detail)
        );
        assert_eq!(
            persistence::active_detail_tab(&shell.ui_store.view.tree),
            Some(WorkspaceTab::Code)
        );
    }

    #[test]
    fn in_flight_manual_rebuild_blocks_export_of_last_good_snapshot() {
        let dir = crate::app::test_support::TestTempDir::new("export-in-flight-rebuild");
        let path = dir.path().join("scene.amx");
        std::fs::write(&path, "#0s\nbox: Rect, size: (100, 100)\n").unwrap();

        let mut shell = GuiShell::load(path, false);
        assert!(shell.document_store.current.is_some());
        assert!(!shell.document_store.snapshot_is_stale());
        assert!(!shell.document_store.showing_last_good());
        assert!(shell.document_store.source.document.last_rebuild_error.is_none());
        assert!(!shell.preview_store.has_pending_rebuild());

        shell.handle_action(Command::Rebuild.into());
        assert!(shell.preview_store.in_flight_rebuild.is_some());

        let effects = shell.handle_action(ShellAction::View(ViewAction::OpenExportDialog));

        assert!(matches!(
            effects.first(),
            Some(Effect::Status(message)) if message.contains("Export unavailable")
        ));
        assert!(!shell.export_store.export_dialog_open);
    }

    #[test]
    fn pending_build_blocks_export_dialog() {
        let mut shell = shell();
        shell.handle_action(Command::EditorChanged.into());
        let effects = shell.handle_action(ShellAction::View(ViewAction::OpenExportDialog));
        assert!(
            matches!(effects.first(), Some(Effect::Status(message)) if message.contains("Export unavailable"))
        );
    }
}
