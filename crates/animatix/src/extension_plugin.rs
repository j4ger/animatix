//! Plugin loader for extension contexts.

use crate::extension_context::ExtensionContext;

/// Disposer returned by a plugin install.
pub type PluginDisposer = Box<dyn FnOnce(&mut ExtensionContext) + Send>;

/// Error returned while installing a plugin.
#[derive(Debug, thiserror::Error)]
#[error("plugin install failed: {0}")]
pub struct PluginError(pub String);

/// A composable extension plugin.
pub trait ExtensionPlugin: Send + Sync {
    /// Stable plugin name.
    fn name(&self) -> &'static str;

    /// Install capabilities into a context and return a disposer.
    fn install(&self, ctx: &mut ExtensionContext) -> Result<PluginDisposer, PluginError>;
}

/// Registry of plugins to install together.
#[derive(Default)]
pub struct PluginLoader {
    plugins: Vec<Box<dyn ExtensionPlugin>>,
}

impl PluginLoader {
    /// Create an empty plugin loader.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin.
    pub fn register(&mut self, plugin: Box<dyn ExtensionPlugin>) {
        self.plugins.push(plugin);
    }

    /// Install all plugins into a context.
    pub fn install_all(
        &self,
        ctx: &mut ExtensionContext,
    ) -> Result<Vec<PluginDisposer>, PluginError> {
        let mut disposers = Vec::new();
        for plugin in &self.plugins {
            let disposer = plugin
                .install(ctx)
                .map_err(|err| PluginError(format!("{}: {}", plugin.name(), err.0)))?;
            disposers.push(disposer);
        }
        Ok(disposers)
    }
}

#[cfg(test)]
mod tests {
    use super::{ExtensionPlugin, PluginError, PluginLoader};
    use crate::extension_context::ExtensionContext;
    use crate::timeline::Value;

    struct DoublePlugin;

    impl ExtensionPlugin for DoublePlugin {
        fn name(&self) -> &'static str {
            "double"
        }

        fn install(
            &self,
            ctx: &mut ExtensionContext,
        ) -> Result<super::PluginDisposer, PluginError> {
            ctx.register_function("double", |args, _env| {
                let Some(Value::Num(n)) = args.first() else {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "double expects one number".to_string(),
                    ));
                };
                Ok(Value::Num(*n * 2.0))
            });
            ctx.provide("plugin", "double");
            Ok(Box::new(|ctx: &mut ExtensionContext| {
                ctx.remove_function("double");
                ctx.remove_service("plugin");
            }))
        }
    }

    #[test]
    fn plugin_loader_installs_and_disposes() {
        let mut loader = PluginLoader::new();
        loader.register(Box::new(DoublePlugin));
        let mut ctx = ExtensionContext::new();
        let disposers = loader.install_all(&mut ctx).expect("install plugins");
        assert!(ctx.get::<&str>("plugin").is_some());

        let mut env = crate::timeline::Environment::new();
        ctx.install_functions(&mut env);
        assert!(matches!(env.get("double"), Some(Value::NativeFn(_))));

        for disposer in disposers {
            disposer(&mut ctx);
        }
        assert!(ctx.get::<&str>("plugin").is_none());
        let mut env = crate::timeline::Environment::new();
        ctx.install_functions(&mut env);
        assert!(env.get("double").is_none());
    }
}
