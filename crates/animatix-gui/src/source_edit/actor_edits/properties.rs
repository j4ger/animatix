//! Actor property edits: set, insert, and remove.

use animatix_syntax::ast::{Expr, Property, Stmt};

use super::super::SourceEditError;
use super::super::apply::{
    canonical_to_source, find_actor_decl_mut, find_assignment_mut, find_prop_mut,
};

pub(in crate::source_edit) fn set_property(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);

    // 1. Try to find an ActorDecl and update its property.
    if let Some(actor_decl) = find_actor_decl_mut(stmts, actor) {
        if let Some(prop) = find_prop_mut(actor_decl, source_prop) {
            prop.value = value.clone();
            return Ok(());
        }
    }

    // 2. Try to find an Assignment statement and update its value.
    if let Some(Stmt::Assignment { value: val, .. }) =
        find_assignment_mut(stmts, actor, source_prop)
    {
        *val = value;
        return Ok(());
    }

    Err(SourceEditError::PropertyNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
    })
}

pub(in crate::source_edit) fn insert_property(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);

    let actor_decl =
        find_actor_decl_mut(stmts, actor).ok_or_else(|| SourceEditError::ActorNotFound {
            actor: actor.to_string(),
        })?;

    // Check if property already exists
    if find_prop_mut(actor_decl, source_prop).is_some() {
        return Err(SourceEditError::PropertyAlreadyExists {
            actor: actor.to_string(),
            property: property.to_string(),
        });
    }

    // Add new property
    if let Stmt::ActorDecl { ty, props, .. } = actor_decl {
        // Text, Math, Code types use generic props; Svg/Image use fixed schemas
        if ty == "Svg" || ty == "Image" {
            return Err(SourceEditError::FixedSchemaUnsupported {
                actor: actor.to_string(),
                ty: ty.clone(),
            });
        }
        props.push(Property {
            name: source_prop.into(),
            value,
            value_span: None,
            trailing_comment: None,
        });
        Ok(())
    } else {
        Err(SourceEditError::ActorNotFound {
            actor: actor.to_string(),
        })
    }
}

pub(in crate::source_edit) fn remove_property(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);

    let actor_decl =
        find_actor_decl_mut(stmts, actor).ok_or_else(|| SourceEditError::ActorNotFound {
            actor: actor.to_string(),
        })?;

    if let Stmt::ActorDecl { props, .. } = actor_decl {
        let before = props.len();
        props.retain(|p| p.name != source_prop);
        if props.len() < before {
            return Ok(());
        }
    }

    Err(SourceEditError::PropertyNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
    })
}
