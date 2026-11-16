//! Validated numeric inputs supplied by an interactive runtime.

use std::collections::BTreeMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::timeline::{Timeline, Value};

/// A validated, immutable snapshot of runtime numeric inputs.
///
/// Create snapshots with [`Timeline::validate_runtime_inputs`], which checks
/// the scene's `config { runtime_inputs: { ... } }` allow-list. An empty
/// snapshot exposes no values and lets frame-time drivers such as `always`
/// assignments and keyframe variable tracks provide their normal values.
#[derive(Clone, Debug, Default)]
pub struct RuntimeInputs {
    pub(crate) values: Option<Arc<BTreeMap<String, Value>>>,
    pub(crate) cache_key: u64,
    pub(crate) schema_key: u64,
    pub(crate) schema_names: Option<Arc<Vec<String>>>,
}

impl RuntimeInputs {
    /// Whether this snapshot has any active external values.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.values.is_none()
    }

    /// A content signature for caches and caller-side frame deduplication.
    #[inline]
    pub fn cache_key(&self) -> u64 {
        self.cache_key
    }
}

/// Error returned when runtime input values do not match the declared input
/// contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeInputError {
    /// The provided name is not listed in `config.runtime_inputs`.
    UnknownName(String),
    /// Runtime inputs accept finite numeric values only.
    NonFiniteValue(String),
    /// A snapshot validated for another input whitelist cannot be reused here.
    IncompatibleTimeline,
}

impl fmt::Display for RuntimeInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownName(name) => {
                write!(f, "runtime input '{name}' is not declared in config.runtime_inputs")
            },
            Self::NonFiniteValue(name) => {
                write!(f, "runtime input '{name}' must be a finite number")
            },
            Self::IncompatibleTimeline => write!(
                f,
                "runtime input snapshot was validated for a different runtime_inputs whitelist"
            ),
        }
    }
}

impl std::error::Error for RuntimeInputError {}

impl Timeline {
    /// Names accepted by [`Self::validate_runtime_inputs`], in sorted order.
    #[inline]
    pub fn runtime_input_names(&self) -> &[String] {
        &self.runtime_input_names
    }

    /// Validate a complete input snapshot against this timeline's declared
    /// whitelist. Missing names are allowed and fall back to the current frame
    /// driver; unknown names and non-finite values reject the whole snapshot.
    pub fn validate_runtime_inputs(
        &self,
        values: &BTreeMap<String, f64>,
    ) -> Result<RuntimeInputs, RuntimeInputError> {
        for (name, value) in values {
            if self.runtime_input_names.binary_search(name).is_err() {
                return Err(RuntimeInputError::UnknownName(name.clone()));
            }
            if !value.is_finite() {
                return Err(RuntimeInputError::NonFiniteValue(name.clone()));
            }
        }

        if values.is_empty() {
            return Ok(RuntimeInputs::default());
        }

        let env_values: BTreeMap<String, Value> =
            values.iter().map(|(name, value)| (name.clone(), Value::Num(*value))).collect();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for (name, value) in values {
            name.hash(&mut hasher);
            value.to_bits().hash(&mut hasher);
        }
        let cache_key = hasher.finish();
        let schema_key = runtime_input_schema_key(&self.runtime_input_names);
        Ok(RuntimeInputs {
            values: Some(Arc::new(env_values)),
            cache_key,
            schema_key,
            schema_names: Some(Arc::new(self.runtime_input_names.clone())),
        })
    }

    /// Verify that this non-empty snapshot was created for the same declared
    /// input schema. The empty snapshot is a universal clear operation.
    pub(crate) fn validate_runtime_input_snapshot(
        &self,
        inputs: &RuntimeInputs,
    ) -> Result<(), RuntimeInputError> {
        if inputs.is_empty() {
            return Ok(());
        }
        if inputs.schema_key != runtime_input_schema_key(&self.runtime_input_names)
            || inputs.schema_names.as_deref() != Some(&self.runtime_input_names)
        {
            return Err(RuntimeInputError::IncompatibleTimeline);
        }
        Ok(())
    }
}

fn runtime_input_schema_key(names: &[String]) -> u64 {
    if names.is_empty() {
        return 0;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    names.hash(&mut hasher);
    hasher.finish()
}
