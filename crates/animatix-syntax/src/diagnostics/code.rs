//! Human-readable rendering of [`DiagnosticCode`].
//!
//! Kept beside `mod.rs` rather than inside it so the diagnostics module stays
//! within the structure gate's mod.rs line budget as new codes are added.

use std::fmt;

use super::DiagnosticCode;

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticCode::SourceLoadFailure => write!(f, "source-load-failure"),
            DiagnosticCode::ParseError => write!(f, "parse-error"),
            DiagnosticCode::RenderFailure => write!(f, "render-failure"),
            DiagnosticCode::UnsupportedModifierKey => write!(f, "unsupported-modifier-key"),
            DiagnosticCode::UnsupportedAssignmentProperty => {
                write!(f, "unsupported-assignment-property")
            },
            DiagnosticCode::InvalidAssignmentTarget => {
                write!(f, "invalid-assignment-target")
            },
            DiagnosticCode::InvalidModifierValue => write!(f, "invalid-modifier-value"),
            DiagnosticCode::AlwaysWriteNotAnimatable => {
                write!(f, "always-write-not-animatable")
            },
            DiagnosticCode::InvalidConfigValue => write!(f, "invalid-config-value"),
            DiagnosticCode::ConflictingModifierKey => write!(f, "conflicting-modifier-key"),
            DiagnosticCode::UnknownAction => write!(f, "unknown-action"),
            DiagnosticCode::UnknownColorscheme => write!(f, "unknown-colorscheme"),
            DiagnosticCode::UnknownColorReference => write!(f, "unknown-color-reference"),
            DiagnosticCode::UnsupportedActionTarget => write!(f, "unsupported-action-target"),
            DiagnosticCode::UnsupportedSequenceStatement => {
                write!(f, "unsupported-sequence-statement")
            },
            DiagnosticCode::UnknownTargetPath => write!(f, "unknown-target-path"),
            DiagnosticCode::UnknownLookupPath => write!(f, "unknown-lookup-path"),
            DiagnosticCode::UnsupportedStaggerStatement => {
                write!(f, "unsupported-stagger-statement")
            },
            DiagnosticCode::MediaLoadFailure => write!(f, "media-load-failure"),
            DiagnosticCode::LayoutSizeFallback => write!(f, "layout-size-fallback"),
            DiagnosticCode::UnsupportedMediaAssignment => write!(f, "unsupported-media-assignment"),
            DiagnosticCode::InvalidColorschemeData => write!(f, "invalid-colorscheme-data"),
            DiagnosticCode::ColorschemeInheritanceCycle => {
                write!(f, "colorscheme-inheritance-cycle")
            },
            DiagnosticCode::ModuleExportEvalError => write!(f, "module-export-eval-error"),
            DiagnosticCode::ModifierCompilationError => write!(f, "modifier-compilation-error"),
            DiagnosticCode::ConflictingPositionBinding => {
                write!(f, "conflicting-position-binding")
            },
            DiagnosticCode::IgnoredOffset => write!(f, "ignored-offset"),
            DiagnosticCode::DuplicateSceneName => write!(f, "duplicate-scene-name"),
            DiagnosticCode::PlayTargetNotFound => write!(f, "play-target-not-found"),
            DiagnosticCode::PlayCycleDetected => write!(f, "play-cycle-detected"),
            DiagnosticCode::MultiplePlayTargets => write!(f, "multiple-play-targets"),
            DiagnosticCode::PlayInsideKeyframe => write!(f, "play-inside-keyframe"),
            DiagnosticCode::NeverRevealed => write!(f, "never-revealed"),
            DiagnosticCode::DurationShorterThanContent => {
                write!(f, "duration-shorter-than-content")
            },
            DiagnosticCode::LoopNotSeamless => write!(f, "loop-not-seamless"),
            DiagnosticCode::UnknownConfigKey => write!(f, "unknown-config-key"),
            DiagnosticCode::MissingGridCols => write!(f, "missing-grid-cols"),
            DiagnosticCode::OrphanScene => write!(f, "orphan-scene"),
            DiagnosticCode::PersistIgnoresDuration => write!(f, "persist-ignores-duration"),
            DiagnosticCode::PersistLayoutManagedChild => write!(f, "persist-layout-managed-child"),
            DiagnosticCode::PersistTargetNotCarried => write!(f, "persist-target-not-carried"),
            DiagnosticCode::CarryAmbiguousPredecessor => write!(f, "carry-ambiguous-predecessor"),
            DiagnosticCode::PersistAfterRemove => write!(f, "persist-after-remove"),
            DiagnosticCode::InvalidPlotFunc => write!(f, "invalid-plot-func"),
            DiagnosticCode::UnknownActorType => write!(f, "unknown-actor-type"),
            DiagnosticCode::TypeMismatch => write!(f, "type-mismatch"),
            DiagnosticCode::ModifierRuntimeError => write!(f, "modifier-runtime-error"),
            DiagnosticCode::UnknownComponentProperty => write!(f, "unknown-component-property"),
            DiagnosticCode::AlwaysOverridesKeyframes => {
                write!(f, "always-overrides-keyframes")
            },
            DiagnosticCode::AbsolutePositionOnLayoutManagedChild => {
                write!(f, "absolute-position-on-layout-managed-child")
            },
            DiagnosticCode::DeprecatedPrimitive => write!(f, "deprecated-primitive"),
            DiagnosticCode::InvalidPropertyValue => write!(f, "invalid-property-value"),
            DiagnosticCode::ReservedLabelPrefix => write!(f, "reserved-label-prefix"),
            DiagnosticCode::BracedPropertySilentDrop => {
                write!(f, "braced-property-silent-drop")
            },
            DiagnosticCode::CalloutTargetNotFound => write!(f, "callout-target-not-found"),
            DiagnosticCode::MissingWildcardArm => write!(f, "missing-wildcard-arm"),
            DiagnosticCode::UnknownTypeAlias => write!(f, "unknown-type-alias"),
            DiagnosticCode::DuplicateLabel => write!(f, "duplicate-label"),
            DiagnosticCode::UnusedLabel => write!(f, "unused-label"),
            DiagnosticCode::UndefinedLabel => write!(f, "undefined-label"),
            DiagnosticCode::UnknownProperty => write!(f, "unknown-property"),
            DiagnosticCode::InapplicableProperty => write!(f, "inapplicable-property"),
            DiagnosticCode::UnimplementedProperty => write!(f, "unimplemented-property"),
            DiagnosticCode::MissingGlyph => write!(f, "missing-glyph"),
            DiagnosticCode::UnsupportedFormatSpecifier => {
                write!(f, "unsupported-format-specifier")
            },
            DiagnosticCode::UnknownType => write!(f, "unknown-type"),
        }
    }
}
