use std::collections::HashMap;

use super::NamespaceType;

impl Default for NamespaceType {
    fn default() -> Self {
        NamespaceType::Namespace(HashMap::new())
    }
}
