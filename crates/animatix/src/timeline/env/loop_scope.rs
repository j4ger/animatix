//! Save only loop-local names so nested loops restore their caller's bindings.

use smallvec::SmallVec;

use super::{Environment, Value};
use crate::ast::LoopPattern;

pub(crate) struct LoopScope<'a> {
    bindings: SmallVec<[(&'a str, Option<Value>); 4]>,
}

impl<'a> LoopScope<'a> {
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|(name, _)| *name)
    }

    pub(crate) fn capture(
        env: &Environment,
        pattern: &'a LoopPattern,
        index: Option<&'a str>,
    ) -> Self {
        let names = match pattern {
            LoopPattern::Single(name) => std::slice::from_ref(name),
            LoopPattern::Tuple(names) => names.as_slice(),
        };
        Self {
            bindings: names
                .iter()
                .map(String::as_str)
                .chain(index)
                .map(|name| (name, env.overrides.get(name).cloned()))
                .collect(),
        }
    }

    pub(crate) fn restore(self, env: &mut Environment) {
        for (name, previous) in self.bindings {
            if let Some(value) = previous {
                env.set(name, value);
            } else {
                env.overrides.remove(name);
                env.mark_mutated();
            }
        }
    }
}
