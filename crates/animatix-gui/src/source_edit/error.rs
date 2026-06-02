//! Structured errors for source editing operations.

/// Errors that can occur when applying a [`SourceEdit`](super::SourceEdit).
#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum SourceEditError {
    /// The requested actor does not exist in the AST.
    #[error("actor '{actor}' not found")]
    ActorNotFound { actor: String },

    /// The requested property does not exist on the actor.
    #[error("property '{property}' not found on actor '{actor}'")]
    PropertyNotFound { actor: String, property: String },

    /// The requested scene does not exist.
    #[error("scene '{scene}' not found")]
    SceneNotFound { scene: String },

    /// The keyframe time is invalid (e.g. non-positive delta).
    #[error("invalid keyframe time: {time_s}s")]
    InvalidKeyframeTime { time_s: f64 },

    /// A scene with this name already exists.
    #[error("scene name '{name}' already exists")]
    DuplicateSceneName { name: String },

    /// The actor uses a fixed property schema (e.g. Svg, Image) and does not support insertion.
    #[error("actor '{actor}' (type '{ty}') uses a fixed schema; properties cannot be inserted")]
    FixedSchemaUnsupported { actor: String, ty: String },

    /// The requested container does not exist.
    #[error("container '{container}' not found")]
    ContainerNotFound { container: String },

    /// The requested parent does not exist.
    #[error("parent '{parent}' not found")]
    ParentNotFound { parent: String },

    /// No keyframe exists at the requested time for the given actor/property.
    #[error("no keyframe for '{actor}.{property}' at {time_s}s")]
    KeyframeNotFound { actor: String, property: String, time_s: f64 },

    /// Nothing was found to rename.
    #[error("nothing to rename from '{old_label}'")]
    NothingToRename { old_label: String },

    /// An empty actor list was provided for an operation that requires at least one.
    #[error("empty actor list")]
    EmptyActorList,

    /// Catch-all for errors that don't fit a specific variant.
    #[error("{0}")]
    Generic(String),
}
