pub mod entrance;
pub mod exit;
pub mod registry;

use crate::ast::Action;
use crate::timeline::Timeline;
use entrance::{FadeIn, WipeIn};
use exit::FadeOut;
use registry::{ActionSignature, BuiltinAction};

/// Returns a list of all registered built-in actions.
fn get_builtin_actions() -> Vec<Box<dyn BuiltinAction>> {
    vec![Box::new(FadeIn), Box::new(WipeIn), Box::new(FadeOut)]
}

/// Looks up the action by verb and executes it if found.
pub fn process_action(action: &Action, time_ms: f64, timeline: &mut Timeline) {
    let actions = get_builtin_actions();
    for builtin in actions {
        if builtin.signature().name == action.verb {
            builtin.execute(action, time_ms, timeline);
            return;
        }
    }
}

/// Exposes all action signatures for LSP/UI integration.
pub fn get_action_signatures() -> Vec<ActionSignature> {
    get_builtin_actions()
        .iter()
        .map(|a| a.signature())
        .collect()
}
