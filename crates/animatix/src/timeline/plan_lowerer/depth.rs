//! Build-time recursion guard for constructing and mounting timeline plans.

use std::cell::Cell;

/// Maximum nested plan construction or mounting depth on one build thread.
pub(crate) const MAX_PLAN_EXPANSION_DEPTH: usize = 16;

thread_local! {
    static PLAN_EXPANSION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// RAII scope for recursive plan construction and mounting.
pub(crate) struct PlanExpansionGuard;

impl PlanExpansionGuard {
    /// Enter one nested plan expansion, returning `None` at the safety limit.
    pub(crate) fn enter() -> Option<Self> {
        PLAN_EXPANSION_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_PLAN_EXPANSION_DEPTH {
                return None;
            }
            depth.set(current + 1);
            Some(Self)
        })
    }
}

impl Drop for PlanExpansionGuard {
    fn drop(&mut self) {
        PLAN_EXPANSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_PLAN_EXPANSION_DEPTH, PlanExpansionGuard};

    fn rejects_next_level(remaining: usize) -> bool {
        if remaining == 0 {
            return PlanExpansionGuard::enter().is_none();
        }
        let Some(_scope) = PlanExpansionGuard::enter() else {
            return false;
        };
        rejects_next_level(remaining - 1)
    }

    #[test]
    fn nested_plan_guard_rejects_the_first_level_over_the_limit_and_resets() {
        assert!(rejects_next_level(MAX_PLAN_EXPANSION_DEPTH));
        assert!(
            PlanExpansionGuard::enter().is_some(),
            "the guard must release all levels after a rejected nested expansion"
        );
    }

    #[test]
    fn recursive_plan_source_stops_with_a_build_diagnostic() {
        let source = r#"
fn recurse() -> Timeline { return #{ recurse() } }
#0s
recurse()
"#;
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "{errors:?}");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast.expect("AST"),
            &std::collections::HashMap::new(),
        );
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code == crate::diagnostics::DiagnosticCode::CallableEvaluationError
                    && diagnostic.message.contains("maximum nesting depth")
            }),
            "recursive plan expansion must stop with a diagnostic: {:?}",
            report.diagnostics
        );
    }
}
