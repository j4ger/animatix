//! # Property Engine
//!
//! Generic functions that replace the N×M match-block explosion.
//!
//! Instead of 7 separate `match prop.name.as_str()` blocks spread across
//! build.rs, assignments.rs, declarations_text.rs, media.rs, and runtime.rs,
//! all property dispatch goes through three functions:
//!
//! 1. `write_property_field()` — write a parsed value to the correct track field
//! 2. `parse_property_value()` — convert an Expr to a PropertyValue by ValueType
//! 3. `inject_property_into_env()` — inject a property's value into the runtime env
//!
//! ## Usage
//!
//! ```ignore
//! let schema = lookup_property("color").unwrap();
//! let value = parse_property_value(schema.value_type, &expr, env, diag, subject);
//! if let Some(value) = value {
//!     write_property_field(track, schema.field, value, t_start_ms, t_end_ms, easing, diag);
//! }
//! ```
//!
//! This replaces what was previously:
//! ```ignore
//! match prop.name.as_str() {
//!     "color" => {
//!         // parse color
//!         // snapshot start if duration > 0
//!         // preserve if delay > 0
//!         // write end keyframe
//!     }
//!     // repeat for every property
//! }
//! ```

mod declaration;
mod plan;
mod read;
#[cfg(test)]
mod tests;
mod value;

pub use declaration::write_property_field;
pub(crate) use declaration::write_property_fields;
pub use plan::{read_property_plan_slot, write_property_plan_slot};
pub use value::{EnumPropertyValue, PropertyValue};

// The pre-split module exposed every typed writer crate-wide. Keep those paths
// intact even where no caller reaches them today.
#[allow(unused_imports)]
pub(crate) use declaration::{
    write_command_list, write_f32, write_f32_list, write_gradient, write_point_list,
    write_shape_type, write_string, write_tagged, write_transform, write_vec2, write_vec4,
};
pub(crate) use plan::{parse_extension_property_value, write_extension_property_slot};
pub(crate) use read::{
    effective_f32_resolved, effective_half_size_resolved, effective_transform,
    inject_extension_properties_into_env, inject_property_into_env,
};
// The pre-split module exposed this crate-wide; keep the path intact even
// though only the `effective_*` readers call it today.
#[allow(unused_imports)]
pub(crate) use read::read_property_value_resolved;
pub(crate) use value::{parse_property_value, values_wrap};
