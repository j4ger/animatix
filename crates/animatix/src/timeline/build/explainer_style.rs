//! Declaration defaults for the opt-in math explainer style.

use std::borrow::Cow;

use super::*;
use crate::ast::Property;

impl Timeline {
    pub(super) fn apply_explainer_style_config(
        &mut self,
        value: &Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match value {
            Expr::Str(name) | Expr::Ident(name) if name == "math-explainer" => {
                self.math_explainer_style = true;
            },
            Expr::Str(name) | Expr::Ident(name) if name == "default" => {
                self.math_explainer_style = false;
            },
            _ => diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidConfigValue,
                    DiagnosticPhase::Build,
                    "Config 'style' expects 'default' or 'math-explainer'.".to_string(),
                )
                .with_subject("style"),
            ),
        }
    }

    pub(super) fn explainer_declaration_props<'a>(
        &self,
        label: &str,
        ty: &str,
        props: &'a [Property],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Cow<'a, [Property]> {
        if !self.math_explainer_style {
            return Cow::Borrowed(props);
        }

        let role_prop = props.iter().find(|prop| prop.name == "role");
        if ty != "Graph" && role_prop.is_none() && self.world.get_id(label).is_some() {
            return Cow::Borrowed(props);
        }
        let role = if let Some(prop) = role_prop {
            let env = self.build_eval_env(time_ms as u64);
            match evaluate_expr_with_lookup_diagnostic(&prop.value, &env, diagnostics, label) {
                Some(Value::Str(role)) => role,
                Some(other) => {
                    warn_invalid_role(label, ty, &format!("{other:?}"), diagnostics);
                    return Cow::Borrowed(props);
                },
                None => return Cow::Borrowed(props), // The evaluator reported the error.
            }
        } else {
            String::new()
        };

        let scale = self.resolution.map_or(1.0, |(w, h)| f64::from(w.min(h)) / 720.0);
        let mut defaults = Vec::new();
        let mut add = |name: &str, value| {
            if !props.iter().any(|prop| prop.name == name) {
                defaults.push(Property::new(name, value));
            }
        };
        if ty == "Graph" {
            let previous = self.coordinate_frame_for(label);
            add("grid", Expr::Bool(previous.is_none_or(|frame| frame.grid)));
            add("ticks", Expr::Bool(previous.is_none_or(|frame| frame.ticks)));
            add(
                "tick_labels",
                Expr::Str(previous.map_or_else(|| "x".into(), |frame| frame.tick_labels.clone())),
            );
            if previous.is_none() && !has_authored_stroke(props) {
                add("stroke", token("text", "muted"));
            }
        } else if ty == "PlotCurve" {
            let (width, color) = match role.as_str() {
                "reference" => (2.5, token("text", "secondary")),
                "focus" | "" => (5.0, token("accent", "primary")),
                _ => {
                    warn_invalid_role(label, ty, &role, diagnostics);
                    return Cow::Borrowed(props);
                },
            };
            add("stroke_width", Expr::Num(width * scale));
            if !has_authored_stroke(props) {
                add("stroke", color);
            }
        } else if matches!(ty, "Text" | "Typst" | "Math" | "Counter") {
            let (size, color) = match role.as_str() {
                "title" => (54.0, token("text", "primary")),
                "body" => (34.0, token("text", "primary")),
                "note" => (24.0, token("text", "secondary")),
                "formula" => (60.0, token("accent", "primary")),
                "metric" => (38.0, token("text", "primary")),
                "" if matches!(ty, "Typst" | "Math") => (42.0, token("text", "primary")),
                "" if ty == "Counter" => (38.0, token("text", "primary")),
                "" => (34.0, token("text", "primary")),
                _ => {
                    warn_invalid_role(label, ty, &role, diagnostics);
                    return Cow::Borrowed(props);
                },
            };
            add("font_size", Expr::Num(size * scale));
            add("color", color);
            if role == "title" {
                add("font_weight", Expr::Str("bold".into()));
            }
        }
        if defaults.is_empty() {
            Cow::Borrowed(props)
        } else {
            defaults.extend_from_slice(props);
            Cow::Owned(defaults)
        }
    }
}

fn token(section: &str, name: &str) -> Expr {
    Expr::Path(vec![section.into(), name.into()])
}

fn has_authored_stroke(props: &[Property]) -> bool {
    props.iter().any(|prop| {
        matches!(prop.name.as_str(), "color" | "stroke" | "stroke_color" | "stroke_gradient")
    })
}

fn warn_invalid_role(label: &str, ty: &str, role: &str, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::InvalidPropertyValue,
            DiagnosticPhase::Build,
            format!("'{label}': {ty} does not support the math-explainer role '{role}'."),
        )
        .with_subject(label),
    );
}
