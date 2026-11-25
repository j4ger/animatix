use animatix_i18n::FluentArgs;
use animatix_syntax::diagnostics::{ParseExpectation, ParsePresentation};

use crate::app::i18n::{Language, text_for};

pub(super) fn message(
    language: Language,
    problem: &ParsePresentation,
    context: &[String],
) -> Option<String> {
    let mut args = FluentArgs::new();
    let id = match problem {
        ParsePresentation::Mismatch { expected, found } => {
            if let Some(token) = found {
                args.set("found", token.as_str());
            }
            if !expected.is_empty() {
                let patterns = expected
                    .iter()
                    .map(|pattern| match pattern {
                        ParseExpectation::Rule(rule) => grammar(language, rule),
                        ParseExpectation::Syntax(token) => token.clone(),
                        ParseExpectation::Any => text_for(language, "diagnostic-parse-any"),
                        ParseExpectation::EndOfInput => text_for(language, "diagnostic-parse-end"),
                    })
                    .collect::<Vec<_>>();
                args.set(
                    "expected",
                    patterns.join(&text_for(language, "diagnostic-parse-separator")),
                );
            }
            match (expected.is_empty(), found.is_some()) {
                (false, true) => "diagnostic-parse-expected-found",
                (false, false) => "diagnostic-parse-expected-end",
                (true, true) => "diagnostic-parse-unexpected-found",
                (true, false) => "diagnostic-parse-unexpected-end",
            }
        },
        ParsePresentation::Rule { id, argument } => {
            if let Some(argument) = argument {
                args.set("value", argument.as_str());
            }
            id
        },
    };
    let message = language.format(id, Some(&args))?;
    if context.is_empty() {
        return Some(message);
    }
    let stack = context.iter().map(|rule| grammar(language, rule)).collect::<Vec<_>>();
    let mut args = FluentArgs::new();
    args.set("message", message);
    args.set("context", stack.join(" > "));
    language.format("diagnostic-parse-context", Some(&args))
}

fn grammar(language: Language, label: &str) -> String {
    let id = match label {
        "actor declaration" => "diagnostic-grammar-actor-declaration",
        "always block" => "diagnostic-grammar-always-block",
        "assignment" => "diagnostic-grammar-assignment",
        "closure let block" => "diagnostic-grammar-closure-let-block",
        "component definition" => "diagnostic-grammar-component-definition",
        "config" => "diagnostic-grammar-config",
        "cue name" => "diagnostic-grammar-cue-name",
        "expression" => "diagnostic-grammar-expression",
        "for loop" => "diagnostic-grammar-for-loop",
        "if statement" => "diagnostic-grammar-if-statement",
        "import" => "diagnostic-grammar-import",
        "keyframe" => "diagnostic-grammar-keyframe",
        "let binding" => "diagnostic-grammar-let-binding",
        "let declaration" => "diagnostic-grammar-let-declaration",
        "match statement" => "diagnostic-grammar-match-statement",
        "modifier list" => "diagnostic-grammar-modifier-list",
        "play statement" => "diagnostic-grammar-play-statement",
        "property" => "diagnostic-grammar-property",
        "property reference" => "diagnostic-grammar-property-reference",
        "reactive binding" => "diagnostic-grammar-reactive-binding",
        "return statement" => "diagnostic-grammar-return-statement",
        "scene declaration" => "diagnostic-grammar-scene-declaration",
        "statement" => "diagnostic-grammar-statement",
        "step marker" => "diagnostic-grammar-step-marker",
        "type alias" => "diagnostic-grammar-type-alias",
        "type constructor" => "diagnostic-grammar-type-constructor",
        _ => return label.to_owned(),
    };
    text_for(language, id)
}
