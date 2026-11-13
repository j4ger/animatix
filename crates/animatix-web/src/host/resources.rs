//! Resource provenance is attached before flattening/component expansion.

use std::path::Path;

use animatix::extension_context::ExtensionContext;
use animatix::timeline::{Environment, EvalError, Value};
use animatix_syntax::ast::{Expr, Property, Stmt};
use animatix_syntax::visit::{AstFolder, walk_fold_stmt};
use url::Url;

pub(super) const RESOLVE_RESOURCE: &str = "__amx_resolve_resource";

pub(super) struct ResourceOrigin {
    base: String,
}

impl ResourceOrigin {
    pub fn new(entry: &str, path: &Path) -> Result<Self, url::ParseError> {
        let base = Url::parse(entry)?.join(&path.to_string_lossy())?;
        Ok(Self {
            base: base.to_string(),
        })
    }

    fn resolve(&self, value: Expr) -> Expr {
        Expr::Call(RESOLVE_RESOURCE.to_string(), vec![Expr::Str(self.base.clone()), value])
    }
}

impl AstFolder for ResourceOrigin {
    fn fold_property(&mut self, mut property: Property) -> Property {
        if matches!(property.name.as_str(), "url" | "transcript") {
            property.value = self.resolve(property.value);
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

pub(super) fn extensions() -> ExtensionContext {
    let mut context = ExtensionContext::new();
    // A fresh registry cannot contain this host-private name. Keep this branch
    // diagnostic rather than panicking if registration validation changes.
    if let Err(error) = context.register_function(RESOLVE_RESOURCE, resolve) {
        tracing::error!("failed to register resource resolver: {error}");
    }
    context
}
