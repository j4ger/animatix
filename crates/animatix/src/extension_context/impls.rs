//! Error formatting for the extension registration failures.

use std::fmt;

use super::{ExtensionRegistrationError, PropertyRegistrationError};

impl fmt::Display for PropertyRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "property '{name}' is already registered"),
        }
    }
}

impl std::error::Error for PropertyRegistrationError {}

impl fmt::Display for ExtensionRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "extension '{name}' is already registered"),
        }
    }
}

impl std::error::Error for ExtensionRegistrationError {}
