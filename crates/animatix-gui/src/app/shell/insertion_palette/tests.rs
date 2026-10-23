use std::sync::Arc;

use animatix::extension_context::ExtensionContext;
use animatix::timeline::actions::registry::{ActionSignature, BuiltinAction};

use super::*;

struct ExtensionPulse;

impl BuiltinAction for ExtensionPulse {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "ext-pulse".to_string(),
            category: "Native".to_string(),
            description: "Extension action".to_string(),
            params: vec![],
            modifiers: vec![],
        }
    }

    fn execute(
        &self,
        _action: &animatix_syntax::ast::Action,
        _time_ms: f64,
        _timeline: &mut animatix::timeline::Timeline,
        _diagnostics: &mut Vec<animatix_syntax::diagnostics::Diagnostic>,
    ) {
    }
}

#[test]
fn populate_includes_extension_actions() {
    let mut ctx = ExtensionContext::new();
    ctx.register_action(Box::new(ExtensionPulse)).expect("register action");
    let (ast, errors) = animatix_syntax::parser::parse_source("#0s\n");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed source");
    let report = animatix::timeline::Timeline::build_with_context(
        &ast,
        &std::collections::HashMap::new(),
        Arc::new(ctx),
    );
    let timeline = report.output;

    let mut palette = InsertionPalette::default();
    palette.populate(Some(&timeline), &std::collections::HashMap::new(), eparts::Theme::default());

    assert!(palette.items.iter().any(|item| {
        matches!(
            &item.kind,
            ItemKind::Action { verb } if verb == "ext-pulse"
        )
    }));
}
