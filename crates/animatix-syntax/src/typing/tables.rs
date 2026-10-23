use super::*;

/// Resolve a property type for a primitive actor type.
///
/// This is a pure lookup in [`known_property_types`], the derived per-actor-type
/// table built from the descriptor rows in `animatix-core` (via
/// `schema::property_specs`). Resolving per actor type is the point: a property
/// that exists on some other actor is not a property of this one, so
/// `rect.code` or `rect.standoff` must not type-check. It also keeps the type
/// the table declares — `color` and `stroke` are `Union[Color, Str]`, not a bare
/// `Color`.
pub fn property_type(actor_type: &str, property: &str) -> Option<Type> {
    known_property_types()
        .get(&(actor_type.to_string(), property.to_string()))
        .cloned()
}

/// Known property types per `(actor_type, property)` pair.
///
/// This is the shared source for the syntax typechecker, analyzer-level
/// property diagnostics, property completions, and [`property_type`] itself:
/// there is one property table, derived from the descriptor rows.
pub fn known_property_types() -> &'static HashMap<(String, String), Type> {
    static CACHE: std::sync::OnceLock<HashMap<(String, String), Type>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = HashMap::new();
        for spec in property_specs() {
            for actor_type in spec.actor_types {
                map.insert((actor_type.to_string(), spec.name.to_string()), spec.ty.clone());
            }
        }
        map
    })
}

/// Known property names per actor type, derived from [`known_property_types`].
///
/// Keeping this derived avoids the analyzer maintaining a second hand-written
/// property registry that can drift from the type checker.
pub fn known_properties() -> &'static HashMap<String, Vec<String>> {
    static CACHE: std::sync::OnceLock<HashMap<String, Vec<String>>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = HashMap::<String, Vec<String>>::new();
        for spec in property_specs() {
            for actor_type in spec.actor_types {
                let properties = map.entry(actor_type.to_string()).or_default();
                if !properties.iter().any(|existing| existing == spec.name) {
                    properties.push(spec.name.to_string());
                }
            }
        }
        map
    })
}
