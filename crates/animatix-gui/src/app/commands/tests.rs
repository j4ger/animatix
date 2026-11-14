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
    fn visual_mutations_do_not_change_source_or_history() {
        let mut shell = shell();
        let before = shell.document_store.source.document.source_text.clone();
        for command in [
            Command::AddScene("unwanted".into()),
            Command::DeleteSelectedActors,
            Command::DuplicateSelectedActors,
            Command::GroupSelectedActors,
            Command::PasteActors,
        ] {
            let effects = shell.handle_action(command.into());
            assert!(
                matches!(effects.first(), Some(Effect::Status(message)) if message.contains("Read-only"))
            );
            assert_eq!(shell.document_store.source.document.source_text, before);
            assert!(shell.document_store.history.undo_stack.is_empty());
        }
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
    fn pending_build_blocks_export_dialog() {
        let mut shell = shell();
        shell.handle_action(Command::EditorChanged.into());
        let effects = shell.handle_action(ShellAction::View(ViewAction::OpenExportDialog));
        assert!(
            matches!(effects.first(), Some(Effect::Status(message)) if message.contains("Export unavailable"))
        );
    }
}
