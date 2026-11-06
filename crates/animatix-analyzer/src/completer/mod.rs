//! Context-aware completion provider.

use animatix_syntax::token::{Token, TokenKind, line_col_to_byte_utf16};

use crate::extension_manifest::ExtensionManifest;
use crate::symbol_table::{LabelKind, SymbolTable};

/// A completion item to suggest to the user.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// The text to display in the completion list.
    pub label: String,
    /// The kind of completion (for icon/color).
    pub kind: CompletionKind,
    /// Additional detail (e.g., type info).
    pub detail: Option<String>,
    /// Documentation string.
    pub documentation: Option<String>,
    /// Text to insert (if different from label).
    pub insert_text: Option<String>,
}

/// The kind of completion item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionKind {
    /// A language keyword (e.g., `let`, `import`).
    Keyword,
    /// A type name (e.g., `Text`, `Rect`).
    Type,
    /// An actor property name (e.g., `content`, `position`).
    Property,
    /// An actor or variable label.
    Label,
    /// An action verb (e.g., `fade-in`, `move`).
    Action,
    /// A literal value (e.g., `true`, `null`).
    Value,
    /// A code snippet/template.
    Snippet,
}

/// The context in which completion is being requested.
enum CompletionContext {
    /// Top-level of the file (keywords, labels, types)
    TopLevel,
    /// After ":" in "label: " (expecting a type name)
    TypePosition,
    /// Inside a property block { ... } (expecting property names)
    PropertyBlock { actor_type: Option<String> },
    /// After an action verb (expecting actor labels)
    ActionTarget,
    /// At a statement's verb slot: start of a line/block, or a word that is
    /// not a known action in verb position (offers action verbs so a typo'd
    /// verb like `fly` can be corrected from the completion list)
    VerbPosition,
    /// Inside a modifier list [ ... ] (expecting modifier names)
    ModifierList,
    /// Inside a property value (after "=" or ":")
    PropertyValue {
        property_name: Option<String>,
        actor_type: Option<String>,
    },
    /// Unknown context
    Unknown,
}

/// Snippet completions for common patterns.
use actions::*;
use properties::*;
use snippets::*;
use values::*;

mod actions;
mod api;
mod context;
mod properties;
mod snippets;
#[cfg(test)]
mod tests;
mod values;
pub use api::completions_at;
pub use properties::property_documentation;
pub use snippets::all_snippets;
