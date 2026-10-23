//! Effect declaration and effect-stage parameter edits.

use animatix_syntax::ast::{Expr, InlineItem, Property, Stmt};
use animatix_syntax::walk::walk_inline_items_mut;

use super::super::SourceEditError;
use super::super::apply::find_actor_decl_mut;

/// Update a parameter (or `enabled`) on an effect stage child of `scope`.
pub(in crate::source_edit) fn set_effect_param(
    stmts: &mut [Stmt],
    scope: &str,
    stage: &str,
    param: &str,
    value: Expr,
) -> Result<(), SourceEditError> {
    let actor_decl =
        find_actor_decl_mut(stmts, scope).ok_or_else(|| SourceEditError::ActorNotFound {
            actor: scope.to_string(),
        })?;
    let Stmt::ActorDecl { children, .. } = actor_decl else {
        return Err(SourceEditError::ActorNotFound {
            actor: scope.to_string(),
        });
    };

    let mut found = false;
    walk_inline_items_mut(children, &mut |item| {
        if let InlineItem::Labeled { label, props, .. } = item
            && label == stage
            && let Some(prop) = props.iter_mut().find(|prop| prop.name == param)
        {
            prop.value = value.clone();
            found = true;
        }
    });

    if found {
        Ok(())
    } else {
        Err(SourceEditError::PropertyNotFound {
            actor: format!("{scope}.{stage}"),
            property: param.to_string(),
        })
    }
}

/// Insert a parameter on an effect stage child of `scope`.
pub(in crate::source_edit) fn insert_effect_param(
    stmts: &mut [Stmt],
    scope: &str,
    stage: &str,
    param: &str,
    value: Expr,
) -> Result<(), SourceEditError> {
    let actor_decl =
        find_actor_decl_mut(stmts, scope).ok_or_else(|| SourceEditError::ActorNotFound {
            actor: scope.to_string(),
        })?;
    let Stmt::ActorDecl { children, .. } = actor_decl else {
        return Err(SourceEditError::ActorNotFound {
            actor: scope.to_string(),
        });
    };

    let mut inserted = false;
    let mut already_exists = false;
    walk_inline_items_mut(children, &mut |item| {
        if let InlineItem::Labeled { label, props, .. } = item
            && label == stage
        {
            if props.iter().any(|prop| prop.name == param) {
                already_exists = true;
            } else {
                props.push(Property {
                    name: param.to_string(),
                    value: value.clone(),
                    value_span: None,
                    trailing_comment: None,
                });
                inserted = true;
            }
        }
    });

    if inserted {
        Ok(())
    } else if already_exists {
        Err(SourceEditError::PropertyAlreadyExists {
            actor: format!("{scope}.{stage}"),
            property: param.to_string(),
        })
    } else {
        Err(SourceEditError::ActorNotFound {
            actor: format!("{scope}.{stage}"),
        })
    }
}

/// Insert an effect declaration into a compositing scope's body.
///
/// Effects are declared before content (`docs/effects.md` §1), so the child is
/// placed after any existing effect declarations and before the first content
/// child. Effect-ness is decided by the shared schema table; a plugin effect
/// already present in the scope is treated as content, which only affects the
/// relative position of the new stage among effects, never its position
/// relative to content.
pub(in crate::source_edit) fn insert_effect(
    stmts: &mut [Stmt],
    scope: &str,
    ty: &str,
    label: &str,
    props: Vec<Property>,
) -> Result<(), SourceEditError> {
    let container_decl =
        find_actor_decl_mut(stmts, scope).ok_or_else(|| SourceEditError::ContainerNotFound {
            container: scope.to_string(),
        })?;
    let Stmt::ActorDecl { children, .. } = container_decl else {
        return Err(SourceEditError::ContainerNotFound {
            container: scope.to_string(),
        });
    };
    let index = children
        .iter()
        .position(|item| !is_effect_child(item))
        .unwrap_or(children.len());
    children.insert(
        index,
        InlineItem::Labeled {
            label: label.into(),
            array_index: None,
            ty: ty.into(),
            props,
            modifiers: vec![],
            children: vec![],
        },
    );
    Ok(())
}

/// `true` when an inline child declares a built-in effect.
fn is_effect_child(item: &InlineItem) -> bool {
    match item {
        InlineItem::Labeled { ty, .. } | InlineItem::Anonymous { ty, .. } => {
            animatix_syntax::schema::effect_spec(ty).is_some()
        },
        _ => false,
    }
}
