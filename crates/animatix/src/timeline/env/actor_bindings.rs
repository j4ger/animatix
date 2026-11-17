use super::*;

type ActorRefs = HashMap<String, crate::timeline::plan_value::ActorRef>;
type ActorArrays = HashMap<String, Vec<(usize, String)>>;

struct ActorSymbols {
    identities: ActorRefs,
    arrays: ActorArrays,
    children: HashMap<String, Vec<String>>,
}

fn parse_array_member(segment: &str) -> Option<(&str, usize)> {
    let (base, suffix) = segment.rsplit_once("__")?;
    if base.is_empty() || suffix.is_empty() || !suffix.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    Some((base, suffix.parse().ok()?))
}

fn collect_actor_symbols(world: &crate::timeline::world::World) -> ActorSymbols {
    let mut identities = HashMap::new();
    let mut arrays: ActorArrays = HashMap::new();
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (label, entity_id) in world.tracks.iter() {
        let reference = world
            .plan_ownership(entity_id)
            .map(|owner| {
                crate::timeline::plan_value::ActorRef::mounted(owner.mount_id(), label.as_str())
            })
            .unwrap_or_else(|| crate::timeline::plan_value::ActorRef::caller(label.as_str()));
        identities.insert(
            label.to_string(),
            reference.with_world_identity(entity_id, world.method_table_id(entity_id)),
        );
        let parts = label.split('.').collect::<Vec<_>>();
        for index in 0..parts.len() {
            if index > 0 {
                let parent = parts[..index].join(".");
                children.entry(parent).or_default().push(parts[index].to_string());
            }
            if let Some((base, array_index)) = parse_array_member(parts[index]) {
                let root = if index == 0 {
                    base.to_string()
                } else {
                    format!("{}.{base}", parts[..index].join("."))
                };
                arrays.entry(root).or_default().push((array_index, parts[..=index].join(".")));
            }
        }
    }
    for members in arrays.values_mut() {
        members.sort_by_key(|(index, _)| *index);
        let mut unique = Vec::<(usize, String)>::with_capacity(members.len());
        for (index, label) in members.drain(..) {
            if let Some((previous_index, previous_label)) = unique.last_mut()
                && *previous_index == index
            {
                *previous_label = label;
                continue;
            }
            unique.push((index, label));
        }
        *members = unique;
    }
    ActorSymbols {
        identities,
        arrays,
        children,
    }
}

fn resolve_actor_symbol(
    label: &str,
    symbols: &ActorSymbols,
    memo: &mut ActorRefs,
) -> Option<crate::timeline::plan_value::ActorRef> {
    if let Some(reference) = memo.get(label) {
        return Some(reference.clone());
    }
    if let Some(reference) = symbols.identities.get(label) {
        let mut members = HashMap::new();
        for segment in symbols.children.get(label).into_iter().flatten() {
            let member = crate::ast::is_array_member_label(segment).unwrap_or(segment);
            let child_label = format!("{label}.{member}");
            if let Some(child) = resolve_actor_symbol(&child_label, symbols, memo) {
                members.insert(member.to_string(), child);
            }
        }
        let resolved = reference.with_children(members);
        memo.insert(label.to_string(), resolved.clone());
        return Some(resolved);
    }
    let members = symbols.arrays.get(label)?;
    let mut candidates = Vec::with_capacity(members.len());
    for (index, member_label) in members {
        if let Some(member) = resolve_actor_symbol(member_label, symbols, memo) {
            candidates.push((*index, member));
        }
    }
    let resolved = crate::timeline::plan_value::ActorRef::resolved_array(label, candidates);
    memo.insert(label.to_string(), resolved.clone());
    Some(resolved)
}

fn resolve_actor_symbols(symbols: &ActorSymbols) -> ActorRefs {
    let mut memo = HashMap::new();
    for label in symbols.identities.keys().chain(symbols.arrays.keys()) {
        let _ = resolve_actor_symbol(label, symbols, &mut memo);
    }
    memo
}

fn install_actor_bindings(env: &mut Environment, bindings: &ActorRefs) {
    let existing = env
        .overrides
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect::<Vec<_>>();
    for (name, value) in existing {
        if let Some(rebound) = rebind_actor_value(&value, bindings) {
            env.set_owned(name, rebound);
        }
    }
    for (label, reference) in bindings {
        if matches!(env.get(label), None | Some(Value::ActorRef(_))) {
            env.set(label, Value::ActorRef(reference.clone()));
        }
    }
}

fn rebind_method_tables(env: &mut Environment, symbols: &ActorSymbols, bindings: &ActorRefs) {
    let method_bindings = symbols
        .identities
        .iter()
        .filter_map(|(label, reference)| {
            reference
                .method_table_id()
                .and_then(|table_id| bindings.get(label).cloned().map(|bound| (table_id, bound)))
        })
        .collect::<Vec<_>>();
    for (table_id, actor) in method_bindings {
        let table_index = table_id.get() as usize;
        let Some(methods) = env.component_method_tables.get(table_index).cloned() else {
            continue;
        };
        let Some(scope) = methods.values().find_map(|value| match value {
            Value::UserFn {
                component_scope: Some(scope),
                ..
            } => Some(scope),
            _ => None,
        }) else {
            continue;
        };
        let mut bound_scope = scope.as_ref().clone();
        bound_scope.bound_actor = Some(actor);
        bound_scope.captured_values =
            Arc::new(bind_captured_values(&bound_scope.captured_values, bindings));
        let bound_scope = Arc::new(bound_scope);
        let rebound = methods
            .iter()
            .map(|(name, value)| (name.clone(), bind_method_scope(value, &bound_scope)))
            .collect();
        Arc::make_mut(&mut env.component_method_tables)[table_index] = Arc::new(rebound);
    }
}

fn bind_captured_values(
    captures: &HashMap<String, HashMap<String, Value>>,
    bindings: &ActorRefs,
) -> HashMap<String, HashMap<String, Value>> {
    captures
        .iter()
        .map(|(method, values)| {
            let values = values
                .iter()
                .map(|(name, value)| {
                    (
                        name.clone(),
                        rebind_actor_value(value, bindings).unwrap_or_else(|| value.clone()),
                    )
                })
                .collect();
            (method.clone(), values)
        })
        .collect()
}

fn bind_method_scope(value: &Value, scope: &Arc<super::ComponentMethodScope>) -> Value {
    let Value::UserFn {
        name,
        params,
        return_type,
        body,
        function_context,
        definition_path,
        component_scope: Some(_),
    } = value
    else {
        return value.clone();
    };
    Value::UserFn {
        name: name.clone(),
        params: Arc::clone(params),
        return_type: return_type.clone(),
        body: Arc::clone(body),
        function_context: function_context.clone(),
        definition_path: definition_path.clone(),
        component_scope: Some(Arc::clone(scope)),
    }
}

impl Environment {
    /// Freeze build-resolved actor handles into the shared environment values.
    /// Mutable state remains exclusively in World component stores.
    pub(crate) fn bind_world_actor_refs(&mut self, world: &crate::timeline::world::World) {
        if self.component_method_tables.is_empty() {
            return;
        }
        let symbols = collect_actor_symbols(world);
        let bindings = resolve_actor_symbols(&symbols);
        install_actor_bindings(self, &bindings);
        rebind_method_tables(self, &symbols, &bindings);
    }
}
