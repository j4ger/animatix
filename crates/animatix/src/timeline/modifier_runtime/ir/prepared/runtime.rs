use super::super::exec::{
    evaluate, execute_loop, execute_stmt, modifier_index, record_override, write_indexed_override,
};
use super::super::fast_program::execute_fast_statements;
use super::{
    Index, IndexPath, LoopBody, ModifierGeometry, ModifierIrProgram, ModifierOverrides,
    PreparedModifiers, Statement, Target, Value, fold,
};
use crate::timeline::frame_env::apply_override_incremental;
use crate::timeline::{Environment, EvalError};

impl PreparedModifiers {
    pub(crate) fn matches(&self, source: &ModifierIrProgram, env: &Environment) -> bool {
        std::ptr::eq(self.source.as_ref(), source)
            && env.let_scopes.borrow().is_empty()
            && self.guards.iter().all(|(name, expected)| {
                env.get_ref(name).is_some_and(|actual| fold::same_value(expected, actual))
            })
    }

    pub(crate) fn execute(
        &self,
        env: &mut Environment,
        overrides: &mut ModifierOverrides,
        geometry: ModifierGeometry<'_>,
    ) -> Result<(), EvalError> {
        execute(&self.statements, env, overrides, geometry)
    }
}

fn execute(
    statements: &[Statement],
    env: &mut Environment,
    overrides: &mut ModifierOverrides,
    geometry: ModifierGeometry<'_>,
) -> Result<(), EvalError> {
    for statement in statements {
        match statement {
            Statement::Generic(stmt) => execute_stmt(stmt, env, overrides, Some(geometry))?,
            Statement::Fast(stmt) => {
                execute_fast_statements(
                    std::slice::from_ref(stmt.as_ref()),
                    env,
                    overrides,
                    Some(geometry),
                )?;
            },
            Statement::Branch { condition, yes, no } => {
                let condition = evaluate(condition, env, Some(geometry), overrides)?;
                execute(if condition.is_truthy() { yes } else { no }, env, overrides, geometry)?;
            },
            Statement::Loop {
                pattern,
                index_var,
                items,
                body,
            } => {
                let items = evaluate(items, env, Some(geometry), overrides)?;
                execute_loop(env, pattern, index_var.as_deref(), items, |env, index| {
                    let statements = match body {
                        LoopBody::Fixed(bodies) => &bodies[index],
                        LoopBody::Dynamic(body) => body,
                    };
                    execute(statements, env, overrides, geometry)
                })?;
            },
            Statement::Indexed {
                base,
                property,
                value,
                index,
            } => {
                let value = evaluate(value, env, Some(geometry), overrides)?;
                let index = match index {
                    Index::Bound(target) => {
                        target.write(env, overrides, geometry, property, value);
                        continue;
                    },
                    Index::Fixed(index) => Some(*index),
                    Index::Dynamic(expr) => {
                        modifier_index(&base.base, evaluate(expr, env, Some(geometry), overrides)?)
                    },
                };
                if let Some(index) = index {
                    base.write(env, overrides, geometry, index, property, value);
                }
            },
        }
    }
    Ok(())
}

impl IndexPath {
    fn write(
        &self,
        env: &mut Environment,
        overrides: &mut ModifierOverrides,
        geometry: ModifierGeometry<'_>,
        index: usize,
        property: &str,
        value: Value,
    ) {
        if let Some(target) = self.targets.get(&index) {
            target.write(env, overrides, geometry, property, value);
        } else {
            write_indexed_override(
                env,
                overrides,
                &self.base,
                self.position,
                index,
                property,
                value,
            );
        }
    }
}

impl Target {
    fn write(
        &self,
        env: &mut Environment,
        overrides: &mut ModifierOverrides,
        geometry: ModifierGeometry<'_>,
        property: &str,
        value: Value,
    ) {
        // An ECS edit may replace an actor. Keep the original authored label
        // when the stored ID no longer resolves to that target.
        let label = geometry
            .timeline
            .world
            .label(self.entity)
            .filter(|label| *label == self.label)
            .unwrap_or(&self.label);
        record_override(overrides, label, property, &value);
        match value {
            Value::Vec2(_) | Value::Color(_) => {
                apply_override_incremental(env, label, property, value)
            },
            value => env.set(&self.env_key, value),
        }
    }
}
