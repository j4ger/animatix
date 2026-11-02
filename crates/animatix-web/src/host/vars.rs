//! Build-time variable overrides: the web seam for the CLI's `--set NAME=VALUE`.
//!
//! The engine owns the only parser — `animatix::timeline::build::defines` turns
//! each `(name, expression text)` pair into a build-environment seed that
//! shadows the file's own `let NAME = …` default. What the web boundary adds is
//! *rendering*: a page hands the player JSON (`data-vars` / `setVars`), so a
//! number, string or boolean has to become expression text before it can join
//! the CLI's channel. These two helpers are that whole rule, and they are
//! target-independent so native `cargo test` covers them without a browser.

/// Animatix expression text for a finite number, or `None` for one with no
/// literal form (NaN / infinity).
///
/// Rust's `f64` `Display` is the shortest round-tripping decimal and never
/// switches to exponent notation, so the text parses back to the same `f64`:
/// `320.0` renders `320` (the lexer's integer form) and `0.5` renders `0.5`.
pub fn number_expr(value: f64) -> Option<String> {
    if !value.is_finite() {
        return None;
    }
    Some(format!("{value}"))
}

/// Animatix string-literal text for `value`, or `None` when the literal grammar
/// cannot hold it faithfully.
///
/// The DSL's literals are raw: the lexer skips an escape pair without
/// interpreting it and nothing unescapes later, so an escaped quote would reach
/// the scene as a backslash *and* a quote. Rendering therefore picks the quote
/// character the string does not contain instead of escaping, and refuses the
/// three shapes that cannot round-trip:
///
/// * a control character — a raw newline ends the literal mid-string;
/// * a backslash — uninterpreted, and a trailing one swallows the closing quote;
/// * both quote characters — no delimiter can close the literal.
///
/// A refused string is a dropped value: the caller logs it and keeps going.
pub fn string_expr(value: &str) -> Option<String> {
    let double = value.contains('"');
    let single = value.contains('\'');
    if value.contains('\\') || (double && single) || value.chars().any(char::is_control) {
        return None;
    }
    let quote = if double { '\'' } else { '"' };
    Some(format!("{quote}{value}{quote}"))
}
