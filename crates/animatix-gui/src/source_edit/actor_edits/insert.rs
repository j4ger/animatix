//! Actor and snippet insertion.

use animatix_syntax::ast::{InlineItem, Property, Stmt};

use super::super::SourceEditError;
use super::super::apply::find_actor_decl_mut;
use super::common::stmt_to_inline_item;

pub(in crate::source_edit) fn insert_actor(
    stmts: &mut Vec<Stmt>,
    ty: &str,
    label: &str,
    props: Vec<Property>,
    container: Option<&str>,
    _time_s: f64,
) -> Result<(), SourceEditError> {
    if let Some(container_label) = container {
        // Insert as a child of the specified container
        let container_decl = find_actor_decl_mut(stmts, container_label).ok_or_else(|| {
            SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            }
        })?;
        if let Stmt::ActorDecl { children, .. } = container_decl {
            children.push(InlineItem::Labeled {
                label: label.into(),
                array_index: None,
                ty: ty.into(),
                props: props.clone(),
                modifiers: vec![],
                children: vec![],
            });
            Ok(())
        } else {
            Err(SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            })
        }
    } else {
        // Insert at top-level
        stmts.push(Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.into(),
            array_index: None,
            ty: ty.into(),
            props,
            modifiers: vec![],
            children: vec![],
            span: None,
        });
        Ok(())
    }
}

/// Insert a parsed snippet (AST fragment) into the statement list.
///
/// If `time_s` is provided, tries to insert inside the keyframe at that time.
/// If `container` is provided, tries to insert as children of that container.
/// Otherwise, appends to the top level.
pub(in crate::source_edit) fn insert_snippet(
    stmts: &mut Vec<Stmt>,
    fragment: Vec<Stmt>,
    time_s: Option<f64>,
    container: Option<&str>,
) -> Result<(), SourceEditError> {
    // If a container is specified, insert as children of that container.
    if let Some(container_label) = container {
        let container_decl = find_actor_decl_mut(stmts, container_label).ok_or_else(|| {
            SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            }
        })?;
        if let Stmt::ActorDecl { children, .. } = container_decl {
            // Convert top-level stmts to inline items and append.
            for stmt in fragment {
                children.push(stmt_to_inline_item(stmt));
            }
            return Ok(());
        } else {
            return Err(SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            });
        }
    }

    // If a time is specified, try to find the keyframe at that time.
    if let Some(target_time) = time_s {
        for stmt in stmts.iter_mut() {
            if let Stmt::Keyframe { time, body, .. } = stmt {
                let kf_time = super::super::apply::time_to_seconds(time);
                if (kf_time - target_time).abs() < 0.001 {
                    body.extend(fragment);
                    return Ok(());
                }
            }
        }
        // No matching keyframe found — create one.
        stmts.push(Stmt::Keyframe {
            time: animatix_syntax::ast::Time::Seconds(target_time),
            body: fragment,
            span: None,
        });
        return Ok(());
    }

    // Default: append to top level.
    stmts.extend(fragment);
    Ok(())
}
