//! # Property Group Handlers
//!
//! Compound property resolution for properties that depend on each other.
//!
//! ## Group resolution lifecycle
//!
//! 1. Declaration processing collects properties by `GroupHandlerId` into a HashMap
//! 2. After all properties are processed, `resolve_groups()` is called once
//! 3. Each group handler receives its collected properties and produces side effects
//!
//! ## Current groups
//!
//! | Group | Properties | Effect |
//! |-------|-----------|--------|
//! | PositionBinding | at, anchor, offset | Resolves to `PositionBinding` + writes position |
//! | VectorShapeState | radius, from, to, start_angle, sweep_angle, points, commands, sides | Builds vector shape paths |
//! | PlotDomain | x_domain, y_domain, t_domain, func, tolerance, max_depth, resolution | Configures plot env |
//! | ContainerLayout | gap, align, cols | Sets container metadata + applies layout |


use super::Expr;
use crate::diagnostics::Diagnostic;
use crate::timeline::property_registry::GroupHandlerId;

/// A collected property for group resolution.
/// Stores the original expression and name.
#[derive(Clone, Debug)]
pub(crate) struct GroupedProperty<'a> {
    pub name: &'a str,
    pub value: &'a Expr,
}

/// Result of group resolution: actions the caller must take on the Timeline.
/// This avoids tight coupling between property_groups and build.rs internals.
#[derive(Clone, Debug)]
pub(crate) enum GroupAction {
    /// Call register_container on the timeline.
    RegisterContainer {
        label: String,
        ty: String,
        gap: f32,
        align: Option<String>,
        cols: Option<usize>,
    },
}

/// Resolve all collected groups for a single actor track.
/// Produces a list of actions the caller (build.rs) must execute.
pub(crate) fn resolve_groups(
    groups: &std::collections::HashMap<GroupHandlerId, Vec<GroupedProperty<'_>>>,
    label: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<GroupAction> {
    let mut actions = Vec::new();

    for (group_id, props) in groups.iter() {
        match group_id {
            GroupHandlerId::ContainerLayout => {
                let mut gap = 0.0f32;
                let mut align: Option<String> = None;
                let mut cols: Option<usize> = None;

                for prop in props {
                    // Simple string-matching here is acceptable because
                    // ContainerLayout only has 3 properties and this will
                    // eventually be replaced by the generic engine.
                    match prop.name {
                        "gap" => {
                            if let Expr::Num(v) = prop.value {
                                gap = *v as f32;
                            }
                        }
                        "align" => {
                            if let Expr::Str(s) = prop.value {
                                align = Some(s.clone());
                            } else if let Expr::Ident(s) = prop.value {
                                align = Some(s.clone());
                            }
                        }
                        "cols" => {
                            if let Expr::Num(v) = prop.value {
                                cols = Some((*v).max(1.0) as usize);
                            }
                        }
                        _ => {}
                    }
                }

                actions.push(GroupAction::RegisterContainer {
                    label: label.to_string(),
                    ty: "Row".to_string(), // caller should override
                    gap,
                    align,
                    cols,
                });
            }
            GroupHandlerId::PositionBinding
            | GroupHandlerId::VectorShapeState
            | GroupHandlerId::PlotDomain => {
                // These groups are currently handled inline in build.rs.
                // When migrating, their handlers will be moved here.
            }
        }
    }

    actions
}
