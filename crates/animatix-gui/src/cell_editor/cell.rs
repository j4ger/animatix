/// A single editable cell in the notebook editor.
#[derive(Clone, Debug)]
pub enum Cell {
    /// Config, import, component definition blocks (collapsible preamble).
    Code { body: String, expanded: bool },

    /// A keyframe declaration with its timestamp, body, and optional leading comment.
    Keyframe {
        /// Raw timestamp text: "0s", "+1.5s", "500ms"
        timestamp: String,
        /// True if this was `#+...` (relative), false for absolute `#...`.
        is_relative: bool,
        /// Absolute time in seconds.
        time_s: f64,
        /// The editable body content (actor declarations, assignments, actions).
        body: String,
        /// Leading comment lines attached to this keyframe (e.g. `// setup scene`).
        attached_comment: Option<String>,
    },
}

impl Cell {
    pub fn is_keyframe(&self) -> bool {
        matches!(self, Self::Keyframe { .. })
    }

    pub fn body(&self) -> &str {
        match self {
            Self::Code { body, .. } | Self::Keyframe { body, .. } => body,
        }
    }

    pub fn body_mut(&mut self) -> &mut String {
        match self {
            Self::Code { body, .. } | Self::Keyframe { body, .. } => body,
        }
    }

    pub fn set_body(&mut self, body: String) {
        *self.body_mut() = body;
    }

    pub fn timestamp_text(&self) -> Option<&str> {
        match self {
            Self::Keyframe { timestamp, .. } => Some(timestamp),
            _ => None,
        }
    }

    pub fn is_relative_timestamp(&self) -> bool {
        matches!(self, Self::Keyframe { is_relative: true, .. })
    }

    pub fn is_expanded(&self) -> bool {
        match self {
            Self::Code { expanded, .. } => *expanded,
            Self::Keyframe { .. } => true,
        }
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        if let Self::Code { expanded: current, .. } = self {
            *current = expanded;
        }
    }

    pub fn attached_comment(&self) -> Option<&str> {
        match self {
            Self::Keyframe { attached_comment, .. } => attached_comment.as_deref(),
            _ => None,
        }
    }

    pub fn set_attached_comment(&mut self, comment: Option<String>) {
        if let Self::Keyframe { attached_comment, .. } = self {
            *attached_comment = comment;
        }
    }

    pub fn time_s(&self) -> Option<f64> {
        match self {
            Self::Keyframe { time_s, .. } => Some(*time_s),
            _ => None,
        }
    }

    pub fn toggle_timestamp_type(&mut self) {
        if let Self::Keyframe { timestamp, is_relative, .. } = self {
            if *is_relative {
                if timestamp.starts_with('+') {
                    timestamp.remove(0);
                }
                *is_relative = false;
            } else {
                if !timestamp.starts_with('+') {
                    timestamp.insert(0, '+');
                }
                *is_relative = true;
            }
        }
    }

    pub fn to_source(&self) -> String {
        match self {
            Self::Code { body, .. } => {
                let mut out = String::with_capacity(body.len() + 1);
                out.push_str(body);
                out.push('\n');
                out
            }
            Self::Keyframe { timestamp, is_relative, body, attached_comment, .. } => {
                let mut out = String::new();

                if let Some(comment) = attached_comment {
                    out.push_str(comment);
                    if !comment.ends_with('\n') {
                        out.push('\n');
                    }
                }

                out.push('#');
                if *is_relative && !timestamp.starts_with('+') {
                    out.push('+');
                }
                if !*is_relative && timestamp.starts_with('+') {
                    out.push_str(timestamp.trim_start_matches('+'));
                } else {
                    out.push_str(timestamp);
                }
                out.push_str(" {\n");
                out.push_str(body);
                if !body.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("}\n");
                out
            }
        }
    }

    pub fn duplicate(&self) -> Self {
        self.clone()
    }
}
