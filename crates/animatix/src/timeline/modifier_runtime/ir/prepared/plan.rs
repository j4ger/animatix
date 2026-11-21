use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::fold::Folder;
use super::{Index, IndexPath, LoopBody, PreparedModifiers, Statement, Target};
use crate::ast::LoopPattern;
use crate::timeline::modifier_runtime::ir::fast_program::FastStmt;
use crate::timeline::modifier_runtime::ir::{CompiledExpr, ModifierIrProgram, ModifierIrStmt};
use crate::timeline::{Timeline, Value};

const MAX_FIXED_ITERATIONS: usize = 64;
const MAX_PLANNED_STATEMENTS: usize = 4096;

pub(crate) fn prepare(
    timeline: &Timeline,
    source: Arc<ModifierIrProgram>,
) -> Option<Arc<PreparedModifiers>> {
    let needs_scene_binding = source.statements.iter().any(needs_preparation);
    // A short straight-line program already gets direct scalar/vector
    // assignment and avoids a second run-time plan.
    if !needs_scene_binding
        && (source.fast.as_ref().is_some_and(|fast| {
            fast.statements.iter().any(|stmt| matches!(stmt, FastStmt::FastAssign(_)))
        }) || !source.statements.iter().any(|stmt| matches!(stmt, ModifierIrStmt::Let { .. })))
    {
        return None;
    }
    let _stage = crate::perf::ScopedStage::new(crate::perf::stage::MODIFIER_PREPARE);
    let mut shadowed = timeline.frame_written_vars.clone();
    collect_names(&source.statements, &mut shadowed);
    let mut planner = Planner {
        timeline,
        folder: Folder::new(timeline, shadowed.clone()),
        paths: HashMap::new(),
        remaining: MAX_PLANNED_STATEMENTS,
    };
    let mut statements = planner.statements(&source.statements, &mut HashMap::new())?;
    let locals = super::slots::compile(timeline, &mut statements, &shadowed);
    if !needs_scene_binding && locals.is_none() {
        return None;
    }
    if locals.is_none()
        && let Some(fast) = &source.fast
    {
        for (prepared, original) in statements.iter_mut().zip(&fast.statements) {
            if matches!(original, FastStmt::FastAssign(_)) {
                *prepared = Statement::Fast(Box::new(original.clone()));
            }
        }
    }
    Some(Arc::new(PreparedModifiers {
        source,
        guards: planner.folder.guards.into_iter().collect(),
        statements,
        locals: locals.unwrap_or_default(),
    }))
}

fn needs_preparation(stmt: &ModifierIrStmt) -> bool {
    match stmt {
        ModifierIrStmt::For { .. } | ModifierIrStmt::AssignIndexed { .. } => true,
        ModifierIrStmt::If {
            then_branch,
            else_branch,
            ..
        } => then_branch.iter().chain(else_branch).any(needs_preparation),
        _ => false, // Straight-line programs already use the scalar fast path.
    }
}

fn pattern_names(pattern: &LoopPattern) -> &[String] {
    match pattern {
        LoopPattern::Single(name) => std::slice::from_ref(name),
        LoopPattern::Tuple(names) => names,
    }
}

fn collect_names(stmts: &[ModifierIrStmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            ModifierIrStmt::Let { name, .. } => {
                names.insert(name.clone());
            },
            ModifierIrStmt::For {
                var,
                index_var,
                body,
                ..
            } => {
                names.extend(pattern_names(var).iter().cloned());
                names.extend(index_var.iter().cloned());
                collect_names(body, names);
            },
            ModifierIrStmt::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_names(then_branch, names);
                collect_names(else_branch, names);
            },
            _ => {}, // Actor property writes do not rebind bare scalar names.
        }
    }
}

struct Planner<'a> {
    timeline: &'a Timeline,
    folder: Folder<'a>,
    paths: HashMap<(String, usize, String), Arc<IndexPath>>,
    remaining: usize,
}

impl Planner<'_> {
    fn statements(
        &mut self,
        statements: &[ModifierIrStmt],
        locals: &mut HashMap<String, Value>,
    ) -> Option<Vec<Statement>> {
        self.remaining = self.remaining.checked_sub(statements.len())?;
        statements.iter().map(|stmt| self.statement(stmt, locals)).collect()
    }

    fn statement(
        &mut self,
        stmt: &ModifierIrStmt,
        locals: &mut HashMap<String, Value>,
    ) -> Option<Statement> {
        let prepared = match stmt {
            ModifierIrStmt::Let { name, value } => {
                let value = self.folder.expression(value, locals);
                if let CompiledExpr::Const(value) = &value {
                    locals.insert(name.clone(), value.clone());
                } else {
                    locals.remove(name);
                }
                Statement::Generic(ModifierIrStmt::Let {
                    name: name.clone(),
                    value,
                })
            },
            ModifierIrStmt::Assign {
                target,
                target_key,
                property,
                value,
            } => {
                // Dotted object writes can also change a binding reached by a subsequent read.
                let value = self.folder.expression(value, locals);
                if target_key.contains('.') {
                    locals.clear();
                }
                Statement::Generic(ModifierIrStmt::Assign {
                    target: target.clone(),
                    target_key: target_key.clone(),
                    property: property.clone(),
                    value,
                })
            },
            ModifierIrStmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.folder.expression(condition, locals);
                let yes = self.statements(then_branch, &mut locals.clone())?;
                let no = self.statements(else_branch, &mut locals.clone())?;
                // An unknown branch may write any live local. Retain only values
                // established by subsequent straight-line bindings.
                locals.clear();
                Statement::Branch { condition, yes, no }
            },
            ModifierIrStmt::For {
                var,
                index_var,
                iterable,
                body,
            } => {
                let items = self.folder.expression(iterable, locals);
                let body = self.loop_body(var, index_var.as_deref(), &items, body, locals)?;
                locals.clear();
                Statement::Loop {
                    pattern: var.clone(),
                    index_var: index_var.clone(),
                    items,
                    body,
                }
            },
            ModifierIrStmt::AssignIndexed {
                base,
                index_position,
                index,
                property,
                value,
            } => {
                let value = self.folder.expression(value, locals);
                let base = self.path(base, *index_position, property);
                let index = match self.folder.expression(index, locals) {
                    CompiledExpr::Const(Value::Num(n))
                        if n.is_finite() && n >= 0.0 && n == n.floor() =>
                    {
                        let index = n as usize;
                        base.targets
                            .get(&index)
                            .map_or(Index::Fixed(index), |target| Index::Bound(Arc::clone(target)))
                    },
                    expression => Index::Dynamic(expression),
                };
                Statement::Indexed {
                    base,
                    property: property.clone(),
                    index,
                    value,
                }
            },
            ModifierIrStmt::Noop => Statement::Generic(ModifierIrStmt::Noop),
        };
        Some(prepared)
    }

    fn loop_body(
        &mut self,
        pattern: &LoopPattern,
        index_var: Option<&str>,
        items: &CompiledExpr,
        body: &[ModifierIrStmt],
        locals: &HashMap<String, Value>,
    ) -> Option<LoopBody> {
        // The executor restores only the iterator bindings. Other names can
        // carry between iterations, so start each specialized body without
        // outer constants for names the loop might assign.
        let mut modified = HashSet::new();
        collect_names(body, &mut modified);
        let mut locals = locals.clone();
        locals.retain(|name, _| !modified.contains(name));
        for name in pattern_names(pattern).iter().map(String::as_str).chain(index_var) {
            locals.remove(name);
        }
        if let LoopPattern::Single(name) = pattern
            && let CompiledExpr::Const(Value::List(values)) = items
            && values.len() <= MAX_FIXED_ITERATIONS
        {
            let mut bodies = Vec::with_capacity(values.len());
            for (index, value) in values.iter().enumerate() {
                let mut iteration = locals.clone();
                iteration.insert(name.clone(), value.clone());
                if let Some(index_var) = index_var {
                    iteration.insert(index_var.to_owned(), Value::Num(index as f64));
                }
                bodies.push(self.statements(body, &mut iteration)?);
            }
            Some(LoopBody::Fixed(bodies))
        } else {
            Some(LoopBody::Dynamic(self.statements(body, &mut locals)?))
        }
    }

    fn path(&mut self, base: &str, position: usize, property: &str) -> Arc<IndexPath> {
        let key = (base.to_owned(), position, property.to_owned());
        Arc::clone(
            self.paths.entry(key).or_insert_with(|| {
                Arc::new(IndexPath::bind(self.timeline, base, position, property))
            }),
        )
    }
}

impl IndexPath {
    fn bind(timeline: &Timeline, base: &str, position: usize, property: &str) -> Self {
        let suffix_at = base.match_indices('.').nth(position).map_or(base.len(), |(i, _)| i);
        let (stem, suffix) = base.split_at(suffix_at);
        let prefix = format!("{stem}__");
        let targets = timeline
            .world
            .tracks
            .iter()
            .filter_map(|(label, entity)| {
                let index_text = label.strip_prefix(&prefix)?.strip_suffix(suffix)?;
                let index = index_text.parse::<usize>().ok()?;
                // The runtime spelling is canonical decimal with no leading zeroes.
                if index.to_string() != index_text {
                    return None;
                }
                let mut env_key = String::new();
                crate::timeline::env_keys::property_into(label, property, &mut env_key);
                Some((
                    index,
                    Arc::new(Target {
                        entity,
                        label: label.to_owned(),
                        env_key,
                    }),
                ))
            })
            .collect();
        Self {
            base: base.to_owned(),
            position,
            targets,
        }
    }
}
