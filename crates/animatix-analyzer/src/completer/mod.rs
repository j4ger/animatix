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
    /// Typed alternative for interfaces that localize the detail.
    pub detail_message: Option<CompletionDetail>,
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
    /// A callable function (built-in, user-defined, or extension-provided).
    Callable,
    /// A named callable parameter.
    Parameter,
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
    TypePosition { parent: Option<String> },
    /// Inside a property block { ... } (expecting property names)
    PropertyBlock { actor_type: Option<String> },
    /// Inside a callable argument list.
    CallArgument {
        callee: Option<String>,
        argument_index: usize,
        current_name: Option<String>,
        supplied_names: Vec<String>,
    },
    /// At a statement or expression position where a call can start.
    CallablePosition,
    /// After a namespace separator, completing that namespace's exported functions.
    NamespacePosition { prefix: String },
    /// After an actor receiver separator, completing its public methods.
    MethodPosition { receiver: String },
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
use callables::*;
use properties::*;
use snippets::*;
use values::*;

mod api;
mod callables;
mod context;
mod detail;
pub use detail::CompletionDetail;
mod properties;
mod snippets;
#[cfg(test)]
mod tests;
mod values;
pub use api::completions_at;
pub(crate) use callables::{callable_for, callable_signature};
pub use properties::property_documentation;
pub use snippets::all_snippets;
