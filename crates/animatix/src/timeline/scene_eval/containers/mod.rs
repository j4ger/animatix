//! Child-pipeline strategies for the container primitives.
//!
//! `Filter`, `Glass`, `Mask`, `Equation` and `Captions` each own a specialized
//! sub-pipeline; the region and bounds helpers every effect scope shares live in
//! [`render`].

mod captions;
mod equation;
mod filter;
mod glass;
mod mask;
mod render;

#[cfg(test)]
pub(crate) use equation::equation_markup_escaped;
