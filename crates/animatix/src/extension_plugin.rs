//! Plugin loader for extension contexts.

#[cfg(feature = "plugin-loading")]
#[path = "extension_native_plugin.rs"]
mod extension_native_plugin;
#[cfg(feature = "plugin-loading")]
pub use extension_native_plugin::NativePlugin;

use crate::extension_context::ExtensionContext;

/// Disposer returned by a plugin install.
///
/// A disposer must be invoked exactly once with the same context the plugin was
/// installed into. Calling it returns the context to its pre-install state.
pub type PluginDisposer = Box<dyn FnOnce(&mut ExtensionContext) + Send>;

/// Error returned while installing a plugin.
#[derive(Debug, thiserror::Error)]
#[error("plugin install failed: {0}")]
pub struct PluginError(pub String);

/// A composable extension plugin.
pub trait ExtensionPlugin: Send + Sync {
    /// Stable plugin name.
    fn name(&self) -> &str;

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
    ///
    /// If any plugin fails, disposers for already-installed plugins are invoked
    /// before returning so the context is left in its pre-install state.
    pub fn install_all(
        &self,
        ctx: &mut ExtensionContext,
    ) -> Result<Vec<PluginDisposer>, PluginError> {
        let mut disposers: Vec<PluginDisposer> = Vec::new();
        for plugin in &self.plugins {
            let disposer = match plugin.install(ctx) {
                Ok(disposer) => disposer,
                Err(err) => {
                    for disposer in disposers.into_iter().rev() {
                        disposer(ctx);
                    }
                    return Err(PluginError(format!("{}: {}", plugin.name(), err.0)));
                },
            };
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
        fn name(&self) -> &str {
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
            })
            .expect("register function");
            ctx.provide("plugin", "double").expect("provide service");
            Ok(Box::new(|ctx: &mut ExtensionContext| {
                ctx.remove_function("double");
                ctx.remove_service("plugin");
            }))
        }
    }

    struct FailPlugin;

    impl ExtensionPlugin for FailPlugin {
        fn name(&self) -> &str {
            "fail"
        }

        fn install(
            &self,
            _ctx: &mut ExtensionContext,
        ) -> Result<super::PluginDisposer, PluginError> {
            Err(PluginError("boom".to_string()))
        }
    }

    struct MarkAction;

    impl crate::timeline::actions::registry::BuiltinAction for MarkAction {
        fn signature(&self) -> crate::timeline::actions::registry::ActionSignature {
            crate::timeline::actions::registry::ActionSignature {
                name: "mark".to_string(),
                category: "Custom".to_string(),
                description: "Test action".to_string(),
                params: vec![],
                modifiers: vec![],
            }
        }

        fn execute(
            &self,
            _action: &crate::ast::Action,
            _time_ms: f64,
            _timeline: &mut crate::timeline::Timeline,
            _diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
        ) {
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

    #[test]
    fn install_all_rolls_back_previous_plugins_on_failure() {
        let mut loader = PluginLoader::new();
        loader.register(Box::new(DoublePlugin));
        loader.register(Box::new(FailPlugin));
        let mut ctx = ExtensionContext::new();

        assert!(loader.install_all(&mut ctx).is_err());
        assert!(ctx.get::<&str>("plugin").is_none());
        let mut env = crate::timeline::Environment::new();
        ctx.install_functions(&mut env);
        assert!(env.get("double").is_none());
    }

    #[test]
    fn duplicate_registrations_are_rejected() {
        let mut ctx = ExtensionContext::new();
        ctx.register_function("double", |args, _env| {
            let Some(Value::Num(n)) = args.first() else {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "double expects one number".to_string(),
                ));
            };
            Ok(Value::Num(*n * 2.0))
        })
        .expect("first function");
        assert!(ctx.register_function("double", |_args, _env| Ok(Value::Num(0.0))).is_err());

        ctx.register_action(Box::new(MarkAction)).expect("first action");
        assert!(ctx.register_action(Box::new(MarkAction)).is_err());

        ctx.provide("theme", "dark").expect("first service");
        assert!(ctx.provide("theme", "light").is_err());
    }
}
