use super::*;

impl PrimitiveInfo {
    /// A baseline identity card for an extension primitive: generic child
    /// processing, no capabilities, display name = type name. Callers mutate
    /// the fields for text-like/shape-like/layout extensions — the same data
    /// the native ABI descriptor carries.
    pub fn extension(type_name: impl Into<Cow<'static, str>>, category: ActorCategory) -> Self {
        let type_name = type_name.into();
        Self {
            type_name: type_name.clone(),
            display_name: type_name,
            category,
            icon_id: Cow::Borrowed("extension"),
            advanced: false,
            capabilities: PrimitiveCapabilities::default(),
            child_processing: ChildProcessingKind::Generic,
            shape: None,
            text: None,
            plot: None,
            stroke_path: false,
            connector: false,
            doc: "",
            default_stub: None,
        }
    }
}

impl PrimitiveInfo {
    /// The authored type name as a `'static` string.
    ///
    /// Always `Some` for catalog rows (their names are string literals) and
    /// `None` for extension rows, whose names are heap-allocated. Callers that
    /// need a `'static` table (the parser's contract tables) use this to skip
    /// extension rows explicitly instead of leaking.
    pub fn static_type_name(&self) -> Option<&'static str> {
        match &self.type_name {
            Cow::Borrowed(name) => Some(name),
            Cow::Owned(_) => None,
        }
    }

    /// The icon glyph as a `'static` string, for built-in rows only (see
    /// [`Self::static_type_name`]).
    pub fn static_icon_id(&self) -> Option<&'static str> {
        match &self.icon_id {
            Cow::Borrowed(icon) => Some(icon),
            Cow::Owned(_) => None,
        }
    }
}
