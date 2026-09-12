//! Bridge layer between the insertion palette and semantic source edits.
//!
//! `InsertionRequest` describes what the user wants to insert.
//! `InsertionContext` provides the document state needed to resolve that
//! request into a concrete `SourceEdit`.
//!
//! This is the ONLY file in the app layer that imports from both
//! `source_edit` and the GUI stores.

use std::collections::HashSet;

use animatix_syntax::ast::Modifier;

/// What the user wants to insert, produced by the palette.
#[derive(Debug, Clone)]
pub enum InsertionRequest {
    /// Insert a primitive actor declaration.
    Primitive {
        type_name: String,
        /// If None, generate a unique label automatically.
        suggested_label: Option<String>,
        /// Property overrides (e.g. component parameter values).
        props: Vec<animatix_syntax::ast::Property>,
    },
    /// Insert an effect declaration into a `Filter` compositing scope.
    Effect {
        type_name: String,
        /// The `Filter` scope that owns the new stage.
        scope: String,
        /// If None, generate a unique label automatically.
        suggested_label: Option<String>,
    },
    /// Insert an action into the current keyframe.
    Action {
        verb: String,
        /// If empty, use the currently selected actor(s).
        targets: Vec<String>,
    },
    /// Insert a raw code snippet.
    #[allow(dead_code)] // Reserved for future raw code snippet insertion
    Snippet { text: String },
}

/// Where the insertion lands.
#[derive(Debug, Clone)]
pub enum InsertionTarget {
    /// Insert at the top level of the current module/scene.
    TopLevel,
    /// Insert inside the keyframe at the exact given time.
    KeyframeBody(f64),
    /// Insert as a child of a container actor.
    IntoContainer(String),
}

/// Read-only snapshot of everything the insertion system needs.
#[derive(Debug, Clone)]
pub struct InsertionContext {
    pub current_time_s: f64,
    pub selected_actors: HashSet<String>,
    pub cursor_cell_time_s: Option<f64>,
    pub selected_container: Option<String>,
}

impl InsertionContext {
    /// Resolve the target time for an action insertion.
    ///
    /// Priority:
    /// 1. Cursor inside a keyframe cell → that cell's time.
    /// 2. Playback head time.
    pub fn resolve_action_time(&self) -> f64 {
        self.cursor_cell_time_s.unwrap_or(self.current_time_s)
    }

    /// Resolve insertion target for a primitive.
    pub fn primitive_target(&self) -> InsertionTarget {
        if let Some(time) = self.cursor_cell_time_s {
            return InsertionTarget::KeyframeBody(time);
        }
        if let Some(container) = self.selected_container.clone() {
            return InsertionTarget::IntoContainer(container);
        }
        InsertionTarget::TopLevel
    }

    /// Resolve insertion target for an action.
    #[allow(dead_code)] // Reserved for future action insertion target resolution
    pub fn action_target(&self) -> Option<InsertionTarget> {
        let time = self.resolve_action_time();
        Some(InsertionTarget::KeyframeBody(time))
    }
}

impl InsertionRequest {
    /// Convert to a `SourceEdit` given current document context.
    pub fn into_source_edit(
        self,
        ctx: &InsertionContext,
    ) -> Option<crate::source_edit::SourceEdit> {
        use crate::source_edit::SourceEdit;

        match self {
            InsertionRequest::Primitive {
                type_name,
                suggested_label,
                props,
            } => {
                let label = suggested_label
                    .unwrap_or_else(|| crate::app::utils::labels::unique_label(None, &type_name));

                match ctx.primitive_target() {
                    InsertionTarget::TopLevel => Some(SourceEdit::InsertActor {
                        ty: type_name,
                        label,
                        props,
                        container: None,
                        time_s: 0.0,
                    }),
                    InsertionTarget::IntoContainer(container) => Some(SourceEdit::InsertActor {
                        ty: type_name,
                        label,
                        props,
                        container: Some(container),
                        time_s: 0.0,
                    }),
                    InsertionTarget::KeyframeBody(time_s) => Some(SourceEdit::InsertActor {
                        ty: type_name,
                        label,
                        props,
                        container: None,
                        time_s,
                    }),
                }
            },
            InsertionRequest::Effect {
                type_name,
                scope,
                suggested_label,
            } => {
                // Effect labels are scoped to the owning `Filter`, so they are
                // resolved against the scope's stages rather than the timeline.
                let label = suggested_label.unwrap_or_else(|| {
                    crate::app::utils::labels::unique_effect_label(None, &scope, &type_name)
                });
                Some(SourceEdit::InsertEffect {
                    scope,
                    ty: type_name,
                    label,
                    props: vec![],
                })
            },
            InsertionRequest::Action { verb, targets } => {
                let targets = if targets.is_empty() {
                    ctx.selected_actors.iter().cloned().collect()
                } else {
                    targets
                };
                if targets.is_empty() {
                    return None;
                }

                let time_s = ctx.resolve_action_time();
                Some(SourceEdit::InsertAction {
                    verb,
                    targets,
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: animatix_syntax::ast::Expr::Ident("1s".into()),
                    }],
                    time_s,
                })
            },
            InsertionRequest::Snippet { text } => {
                // Parse snippet into AST fragment and insert via SourceEdit.
                animatix_syntax::parser::parse_snippet(&text).map(|stmts| {
                    let time_s = ctx.cursor_cell_time_s.or(Some(ctx.current_time_s));
                    let container = ctx.selected_container.clone();
                    SourceEdit::InsertSnippet {
                        stmts,
                        time_s,
                        container,
                    }
                })
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{InsertionContext, InsertionRequest};
    use crate::source_edit::SourceEdit;

    fn context() -> InsertionContext {
        InsertionContext {
            current_time_s: 0.0,
            selected_actors: HashSet::new(),
            cursor_cell_time_s: None,
            selected_container: None,
        }
    }

    #[test]
    fn effect_request_inserts_into_the_given_scope() {
        let edit = InsertionRequest::Effect {
            type_name: "Blur".into(),
            scope: "bg".into(),
            suggested_label: Some("soft1".into()),
        }
        .into_source_edit(&context());
        match edit {
            Some(SourceEdit::InsertEffect {
                scope,
                ty,
                label,
                props,
            }) => {
                assert_eq!(scope, "bg");
                assert_eq!(ty, "Blur");
                assert_eq!(label, "soft1");
                assert!(props.is_empty());
            },
            other => panic!("expected InsertEffect, got {other:?}"),
        }
    }

    #[test]
    fn effect_request_derives_a_label_when_unsuggested() {
        let edit = InsertionRequest::Effect {
            type_name: "ColorGrade".into(),
            scope: "bg".into(),
            suggested_label: None,
        }
        .into_source_edit(&context());
        match edit {
            Some(SourceEdit::InsertEffect { label, .. }) => assert_eq!(label, "colorgrade1"),
            other => panic!("expected InsertEffect, got {other:?}"),
        }
    }
}
