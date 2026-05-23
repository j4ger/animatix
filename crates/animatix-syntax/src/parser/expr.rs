//!
//! Expression atom parsers for the Animatix DSL.
//!
//! The atom parsers (`num`, `percent`, `str_val`, `bool_val`, `null_val`, `time`)
//! are defined inline in [`super::parser()`] because chumsky combinator types require
//! concrete `Copy` types for multi-use patterns in `choice()` and `.map()` chains.
//!
//! This module exists as the future extraction target once chumsky's `Boxed` type
//! (which implements `Clone` via `Rc`) can be used throughout without breaking
//! the existing combinator patterns.