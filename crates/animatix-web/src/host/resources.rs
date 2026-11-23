//! Resource provenance is attached before flattening/component expansion.

use std::path::Path;

use animatix::extension_context::ExtensionContext;
use animatix::timeline::{Environment, EvalError, Value};
use animatix_syntax::ast::{Expr, Property, Stmt};
use animatix_syntax::visit::{AstFolder, walk_fold_stmt};
use url::Url;

pub(super) const RESOLVE_RESOURCE: &str = "__host::resolve_resource";
const RESOLVE_RESOURCES: &str = "__host::resolve_resources";

pub(super) struct ResourceOrigin {
    base: String,
    project: String,
}

impl ResourceOrigin {
    pub fn new(entry: &str, path: &Path) -> Result<Self, url::ParseError> {
        let base = Url::parse(entry)?.join(&path.to_string_lossy())?;
        Ok(Self {
            base: base.to_string(),
            project: entry.to_string(),
        })
    }

    fn resolve(&self, value: Expr) -> Expr {
        Expr::call(RESOLVE_RESOURCE, vec![Expr::Str(self.base.clone()), value])
    }
}

impl AstFolder for ResourceOrigin {
    fn fold_property(&mut self, mut property: Property) -> Property {
        match property.name.as_str() {
            "url" | "transcript" => property.value = self.resolve(property.value),
            "text_file" | "diff_file" => {
                property.value = Expr::call(
                    RESOLVE_RESOURCE,
                    vec![Expr::Str(self.project.clone()), property.value],
                );
            },
            "font_files" | "typst_modules" => {
                property.value = Expr::call(
                    RESOLVE_RESOURCES,
                    vec![
                        Expr::Str(self.project.clone()),
                        property.value,
                        Expr::Bool(property.name == "typst_modules"),
                    ],
                );
            },
            _ => {}, // Other properties have no resource provenance.
        }
        property
    }

    fn fold_stmt(&mut self, statement: Stmt) -> Stmt {
        let mut statement = walk_fold_stmt(self, statement);
        if let Stmt::Assignment {
            property, value, ..
        } = &mut statement
            && matches!(property.as_str(), "url" | "transcript")
        {
            *value = self.resolve(value.clone());
        }
        statement
    }
}

fn resolve(args: &[Value], _: &Environment) -> Result<Value, EvalError> {
    let [Value::Str(base), Value::Str(path)] = args else {
        return Err(EvalError::TypeMismatch("asset URL must evaluate to a string".to_string()));
    };
    Url::parse(base)
        .and_then(|base| base.join(path))
        .map(|url| Value::Str(url.to_string()))
        .map_err(|error| EvalError::TypeMismatch(format!("invalid asset URL: {error}")))
}

fn resolve_list(args: &[Value], env: &Environment) -> Result<Value, EvalError> {
    let [base, Value::List(items), Value::Bool(named)] = args else {
        return Err(EvalError::TypeMismatch("expected a resource list".into()));
    };
    let items = items
        .iter()
        .map(|item| {
            if *named {
                if let Value::List(pair) = item
                    && let [Value::Str(name), file] = pair.as_ref()
                {
                    let file = resolve(&[base.clone(), file.clone()], env)?;
                    return Ok(Value::List(vec![Value::Str(name.clone()), file].into()));
                }
                return Err(EvalError::TypeMismatch(
                    "expected {virtual_name, file} in typst_modules".into(),
                ));
            }
            resolve(&[base.clone(), item.clone()], env)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::List(items.into()))
}

pub(super) fn extensions() -> ExtensionContext {
    use animatix_core::callable::{CallableDescriptor, CallableParam, CallablePhase, CallableType};
    let mut context = ExtensionContext::new();
    // A fresh registry cannot contain this host-private name. Keep this branch
    // diagnostic rather than panicking if registration validation changes.
    let descriptor = CallableDescriptor {
        namespace: "__host".into(),
        name: "resolve_resource".into(),
        params: vec![
            animatix_core::callable::CallableParam::required(
                "base",
                animatix_core::callable::CallableType::Str,
            ),
            animatix_core::callable::CallableParam::required(
                "path",
                animatix_core::callable::CallableType::Str,
            ),
        ],
        return_type: animatix_core::callable::CallableType::Str,
        phase: animatix_core::callable::CallablePhase::Build,
        help: "Resolve a source-relative resource URL".into(),
        category: "Host".into(),
    };
    let list_descriptor = CallableDescriptor {
        name: "resolve_resources".into(),
        params: vec![
            CallableParam::required("base", CallableType::Str),
            CallableParam::required("resources", CallableType::Any),
            CallableParam::required("named", CallableType::Bool),
        ],
        return_type: CallableType::Any,
        phase: CallablePhase::Build,
        help: "Resolve source-relative resource lists".into(),
        ..descriptor.clone()
    };
    if let Err(error) = context.register_callable(descriptor, resolve) {
        tracing::error!("failed to register resource resolver: {error}");
    }
    if let Err(error) = context.register_callable(list_descriptor, resolve_list) {
        tracing::error!("failed to register resource list resolver: {error}");
    }
    context
}
