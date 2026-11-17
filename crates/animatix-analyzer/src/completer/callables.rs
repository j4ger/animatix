use animatix_core::callable::{CallableDescriptor, CallableParam, CallablePhase, CallableType};
use animatix_syntax::ast::{Stmt, TypeAnnotation};

use super::*;

pub(super) fn callable_completions(
    symbols: &SymbolTable,
    manifest: &ExtensionManifest,
) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for callable in animatix_std::callable::callables().iter().chain(manifest.callables.iter()) {
        let label = if callable.namespace == "std" {
            callable.name.clone()
        } else {
            callable.qualified_name()
        };
        let insert_name = if callable.namespace == "std" {
            callable.name.clone()
        } else {
            callable.qualified_name()
        };
        items.push(CompletionItem {
            label,
            kind: CompletionKind::Callable,
            detail: Some(signature(callable)),
            documentation: Some(callable.help.clone()),
            insert_text: Some(format!("{insert_name}(")),
        });
    }
    for name in function_names(symbols) {
        if items.iter().any(|item| item.label == name) {
            continue;
        }
        let descriptor = callable_for(&name, symbols, manifest);
        items.push(CompletionItem {
            label: name.clone(),
            kind: CompletionKind::Callable,
            detail: descriptor.as_ref().map(signature).or_else(|| Some("Function".to_string())),
            documentation: descriptor.as_ref().map(|callable| callable.help.clone()),
            insert_text: Some(format!("{name}(")),
        });
    }
    items
}

pub(super) fn method_completions(symbols: &SymbolTable, receiver: &str) -> Vec<CompletionItem> {
    let Some(actor_type) = symbols.labels.get(receiver).and_then(|label| label.ty.as_deref())
    else {
        return Vec::new();
    };
    let Some(component) = symbols.component_defs.get(actor_type) else {
        return Vec::new();
    };
    component
        .body
        .iter()
        .filter_map(|stmt| {
            let Stmt::FnDecl {
                is_pub: true,
                name,
                params,
                return_type,
                ..
            } = stmt
            else {
                return None;
            };
            let info = animatix_syntax::symbol_table::FunctionInfo {
                name: name.clone(),
                params: params.clone(),
                return_type: return_type.clone(),
                span: None,
                is_pub: true,
            };
            let descriptor = from_function_info("", name, &info);
            Some(CompletionItem {
                label: name.clone(),
                kind: CompletionKind::Callable,
                detail: Some(signature(&descriptor)),
                documentation: None,
                insert_text: Some(format!("{name}(")),
            })
        })
        .collect()
}

pub(super) fn argument_completions(
    symbols: &SymbolTable,
    manifest: &ExtensionManifest,
    callee: Option<&str>,
    argument_index: usize,
    current_name: Option<&str>,
    supplied_names: &[String],
) -> Vec<CompletionItem> {
    let Some(callee) = callee else {
        return label_completions(symbols);
    };
    let Some(callable) = callable_for(callee, symbols, manifest) else {
        return label_completions(symbols);
    };
    let parameter = current_name
        .and_then(|name| callable.params.iter().find(|param| param.name == name))
        .or_else(|| {
            current_name
                .is_none()
                .then(|| callable.params.iter().filter(|param| !param.variadic).nth(argument_index))
                .flatten()
        });
    let mut items = Vec::new();
    for param in &callable.params {
        if supplied_names.iter().any(|name| name == &param.name)
            || current_name == Some(param.name.as_str())
        {
            continue;
        }
        items.push(CompletionItem {
            label: param.name.clone(),
            kind: CompletionKind::Parameter,
            detail: Some(type_name(&param.ty)),
            documentation: (!param.help.is_empty()).then(|| param.help.clone()),
            insert_text: Some(format!("{}: ${{1}}", param.name)),
        });
    }
    if parameter.is_some_and(|param| accepts_actor(&param.ty)) {
        items.extend(actor_completions(symbols));
    } else if matches!(parameter.map(|param| &param.ty), Some(CallableType::Bool)) {
        items.extend(value_completions());
    }
    items
}

pub(super) fn modifier_completions() -> Vec<CompletionItem> {
    [
        ("delay", "Delay before this property transition starts."),
        ("ease", "Easing curve for this property transition."),
        ("duration", "Property transition duration."),
        ("step", "Build-time time advance for each loop iteration."),
    ]
    .into_iter()
    .map(|(name, help)| CompletionItem {
        label: name.to_string(),
        kind: CompletionKind::Parameter,
        detail: Some("Modifier".to_string()),
        documentation: Some(help.to_string()),
        insert_text: Some(format!("{name}: ")),
    })
    .collect()
}

pub(crate) fn callable_for(
    callee: &str,
    symbols: &SymbolTable,
    manifest: &ExtensionManifest,
) -> Option<CallableDescriptor> {
    let callee = callee.replace('.', "::");
    if let Some(callable) = animatix_std::callable::callables().iter().find(|callable| {
        callable.qualified_name() == callee
            || (callee == callable.name && callable.namespace == "std")
    }) {
        return Some(callable.clone());
    }
    if let Some(callable) =
        manifest.callables.iter().find(|callable| callable.qualified_name() == callee)
    {
        return Some(callable.clone());
    }
    if let Some(info) = symbols.resolve_namespaced_function(&callee) {
        let (namespace, name) = callee.rsplit_once("::").unwrap_or(("", callee.as_str()));
        return Some(from_function_info(namespace, name, info));
    }
    if !callee.contains("::")
        && let Some(info) = symbols.function_defs.get(&callee)
    {
        return Some(from_function_info("", &callee, info));
    }
    if let Some((receiver, method)) = callee.rsplit_once("::")
        && let Some(actor_type) = symbols.labels.get(receiver).and_then(|label| label.ty.as_deref())
        && let Some(component) = symbols.component_defs.get(actor_type)
        && let Some(Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            ..
        }) = component
            .body
            .iter()
            .find(|stmt| matches!(stmt, Stmt::FnDecl { name, .. } if name == method))
        && *is_pub
    {
        let info = animatix_syntax::symbol_table::FunctionInfo {
            name: name.clone(),
            params: params.clone(),
            return_type: return_type.clone(),
            span: None,
            is_pub: true,
        };
        return Some(from_function_info("", method, &info));
    }
    None
}

pub(crate) fn callable_signature(callable: &CallableDescriptor) -> String {
    signature(callable)
}

fn function_names(symbols: &SymbolTable) -> Vec<String> {
    fn visit(table: &SymbolTable, prefix: &str, names: &mut Vec<String>) {
        for name in &table.functions {
            let qualified = if prefix.is_empty() || name.contains("::") {
                name.clone()
            } else {
                format!("{prefix}::{name}")
            };
            if !names.contains(&qualified) {
                names.push(qualified);
            }
        }
        for (namespace, nested) in &table.namespaces {
            let next = if prefix.is_empty() {
                namespace.clone()
            } else {
                format!("{prefix}::{namespace}")
            };
            visit(nested, &next, names);
        }
    }
    let mut names = Vec::new();
    visit(symbols, "", &mut names);
    names.sort();
    names
}

fn from_function_info(
    namespace: &str,
    name: &str,
    info: &animatix_syntax::symbol_table::FunctionInfo,
) -> CallableDescriptor {
    from_function_parts(namespace, name, &info.params, info.return_type.as_ref())
}

fn from_function_parts(
    namespace: &str,
    name: &str,
    params: &[animatix_syntax::ast::ParamDef],
    result: Option<&TypeAnnotation>,
) -> CallableDescriptor {
    let params = params
        .iter()
        .map(|param| CallableParam {
            name: param.name.clone(),
            ty: param.param_type.as_ref().map_or(CallableType::Any, from_type_annotation),
            default: None,
            optional: param.default.is_some(),
            variadic: false,
            help: String::new(),
        })
        .collect();
    let return_type = result.map_or(CallableType::Any, from_type_annotation);
    let phase = if return_type == CallableType::Timeline {
        CallablePhase::Build
    } else {
        CallablePhase::BuildAndFrame
    };
    CallableDescriptor {
        namespace: namespace.to_string(),
        name: name.to_string(),
        params,
        return_type,
        phase,
        help: String::new(),
        category: "User".to_string(),
    }
}

fn from_type_annotation(ty: &TypeAnnotation) -> CallableType {
    match ty {
        TypeAnnotation::Any | TypeAnnotation::Alias(_) => CallableType::Any,
        TypeAnnotation::Num => CallableType::Num,
        TypeAnnotation::Duration => CallableType::Duration,
        TypeAnnotation::Timeline => CallableType::Timeline,
        TypeAnnotation::Unit => CallableType::Unit,
        TypeAnnotation::Str => CallableType::Str,
        TypeAnnotation::Bool => CallableType::Bool,
        TypeAnnotation::Vec2 => CallableType::Vec2,
        TypeAnnotation::Vec3 => CallableType::Vec3,
        TypeAnnotation::Vec4 => CallableType::Vec4,
        TypeAnnotation::Color => CallableType::Color,
        TypeAnnotation::Easing => CallableType::Easing,
        TypeAnnotation::Actor => CallableType::Actor,
        TypeAnnotation::Scene => CallableType::Scene,
        TypeAnnotation::List(item) => CallableType::List(Box::new(from_type_annotation(item))),
        TypeAnnotation::Tuple(items) => {
            CallableType::Tuple(items.iter().map(from_type_annotation).collect())
        },
        TypeAnnotation::Function { params, ret } => CallableType::Function {
            params: params
                .iter()
                .enumerate()
                .map(|(index, ty)| (format!("arg{index}"), from_type_annotation(ty)))
                .collect(),
            ret: Box::new(from_type_annotation(ret)),
        },
        TypeAnnotation::Union(items) => {
            CallableType::Union(items.iter().map(from_type_annotation).collect())
        },
        TypeAnnotation::Enum(values) => CallableType::Enum(values.clone()),
    }
}

fn signature(callable: &CallableDescriptor) -> String {
    let params = callable
        .params
        .iter()
        .map(|param| {
            let optional = if param.optional { "?" } else { "" };
            format!("{}{}: {}", param.name, optional, type_name(&param.ty))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "{}({}) → {}",
        callable.qualified_name(),
        params,
        type_name(&callable.return_type)
    )
}

fn accepts_actor(ty: &CallableType) -> bool {
    match ty {
        CallableType::Actor => true,
        CallableType::List(item) => accepts_actor(item),
        CallableType::Union(items) => items.iter().any(accepts_actor),
        _ => false,
    }
}

fn type_name(ty: &CallableType) -> String {
    match ty {
        CallableType::Any => "Any".into(),
        CallableType::Num => "Num".into(),
        CallableType::Bool => "Bool".into(),
        CallableType::Str => "Str".into(),
        CallableType::Vec2 => "Vec2".into(),
        CallableType::Vec3 => "Vec3".into(),
        CallableType::Vec4 => "Vec4".into(),
        CallableType::Color => "Color".into(),
        CallableType::Duration => "Duration".into(),
        CallableType::Timeline => "Timeline".into(),
        CallableType::Unit => "Unit".into(),
        CallableType::Actor => "Actor".into(),
        CallableType::Scene => "Scene".into(),
        CallableType::Easing => "Easing".into(),
        CallableType::List(item) => format!("List[{}]", type_name(item)),
        CallableType::Tuple(items) => {
            format!("Tuple({})", items.iter().map(type_name).collect::<Vec<_>>().join(", "))
        },
        CallableType::Union(items) => items.iter().map(type_name).collect::<Vec<_>>().join(" | "),
        CallableType::Enum(items) => format!("Enum({})", items.join(", ")),
        CallableType::Function { .. } => "Function".into(),
    }
}

pub(super) fn actor_completions(symbols: &SymbolTable) -> Vec<CompletionItem> {
    symbols
        .labels
        .iter()
        .filter(|(_, info)| info.kind == LabelKind::Actor)
        .map(|(name, info)| CompletionItem {
            label: name.clone(),
            kind: CompletionKind::Label,
            detail: info.ty.as_ref().map(|ty| format!("Actor: {ty}")),
            documentation: None,
            insert_text: None,
        })
        .collect()
}
