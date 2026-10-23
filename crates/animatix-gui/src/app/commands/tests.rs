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
