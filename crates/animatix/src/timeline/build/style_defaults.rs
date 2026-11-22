//! Resolve scene style defaults once, before an actor writes its ordinary tracks.

use std::borrow::Cow;

use animatix_core::roles::{ROLE_ACTORS, ROLES, Role, STYLE_TOKENS};

use super::*;
use crate::ast::Property;

#[derive(Clone, Copy, Default)]
pub(crate) enum StylePreset {
    #[default]
    Default,
    MathExplainer,
    Editorial,
    Compact,
}

#[derive(Clone, Copy)]
struct Typography {
    size: f64,
    weight: Option<f64>,
    ink: (&'static str, &'static str),
}

impl StylePreset {
    fn typography(self, role: Role) -> Typography {
        use Role::*;
        let (size, weight, ink) = match role {
            Title => (54.0, Some(700.0), ("text", "primary")),
            Body => (34.0, None, ("text", "primary")),
            Note => (24.0, None, ("text", "secondary")),
            Formula => (60.0, None, ("accent", "primary")),
            Metric => (38.0, None, ("text", "primary")),
            Eyebrow => (24.0, Some(700.0), ("accent", "primary")),
            Reference | Focus => unreachable!("curve roles use stroke defaults"),
        };
        let (size, weight) = match (self, role) {
            (Self::Editorial, Title) => (58.0, Some(750.0)),
            (Self::Editorial, Body) => (32.0, None),
            (Self::Editorial, Metric) => (64.0, Some(700.0)),
            (Self::Compact, Title) => (46.0, Some(700.0)),
            (Self::Compact, Body) => (28.0, None),
            (Self::Compact, Formula) => (48.0, None),
            (Self::Compact, Metric) => (48.0, Some(700.0)),
            (Self::Compact, Eyebrow) => (21.0, weight),
            _ => (size, weight),
        };
        Typography { size, weight, ink }
    }

    pub(crate) fn graph_labels_at_edge(self) -> bool {
        matches!(self, Self::Editorial | Self::Compact)
    }

    pub(crate) fn graph_padding(self) -> Option<f64> {
        match self {
            Self::Default => None,
            Self::MathExplainer => Some(22.0),
            Self::Editorial => Some(32.0),
            Self::Compact => Some(26.0),
        }
    }

    pub(crate) fn graph_label_size(self) -> Option<f64> {
        match self {
            Self::Default => None,
            Self::MathExplainer => Some(26.0),
            Self::Editorial => Some(32.0),
            Self::Compact => Some(29.0),
        }
    }

    fn spacing(self) -> [f64; 4] {
        match self {
            Self::Compact => [40.0, 18.0, 14.0, 12.0],
            _ => [56.0, 24.0, 20.0, 18.0],
        }
    }
}

impl Timeline {
    pub(super) fn apply_style_config(&mut self, value: &Expr, diagnostics: &mut Vec<Diagnostic>) {
        let name = match value {
            Expr::Str(name) | Expr::Ident(name) => name.as_str(),
            _ => "",
        };
        self.style_preset = match name {
            "default" => StylePreset::Default,
            "math-explainer" => StylePreset::MathExplainer,
            "editorial" => StylePreset::Editorial,
            "compact" => StylePreset::Compact,
            _ => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidConfigValue,
                        DiagnosticPhase::Build,
                        "Config 'style' expects 'default', 'editorial', 'compact', or 'math-explainer'.",
                    )
                    .with_subject("style"),
                );
                return;
            },
        };
    }

    pub(crate) fn style_scale(&self) -> f64 {
        self.resolution.map_or(1.0, |(w, h)| {
            let scale = f64::from(w.min(h)) / 720.0;
            if matches!(self.style_preset, StylePreset::Editorial | StylePreset::Compact)
                && f64::from(w) < 1.25 * f64::from(h)
            {
                scale * 0.80
            } else {
                scale
            }
        })
    }

    pub(crate) fn seed_role_values(&mut self) {
        for &(_, name) in ROLES {
            self.env.set_host_global(&format!("roles::{name}"), Value::Str(name.into()));
        }
    }

    pub(crate) fn seed_style_values(&mut self) {
        let scale = self.style_scale();
        for (name, value) in STYLE_TOKENS.iter().zip(self.style_preset.spacing()) {
            self.env.set_host_global(&format!("theme.{name}"), Value::Num(value * scale));
        }
    }

    pub(super) fn style_declaration_props<'a>(
        &self,
        label: &str,
        ty: &str,
        props: &'a [Property],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Cow<'a, [Property]> {
        let role_prop = if ROLE_ACTORS.contains(&ty) {
            props.iter().rev().find(|prop| prop.name == "role")
        } else {
            None
        };
        if ty != "Graph" && role_prop.is_none() && self.world.get_id(label).is_some() {
            return Cow::Borrowed(props);
        }
        let selection = role_prop.and_then(|prop| {
            let env = self.build_eval_env(time_ms as u64);
            let value = match &prop.value {
                Expr::Ident(name) => match env.get(name) {
                    Some(Value::ActorRef(_)) | None => Some(Value::Str(name.clone())),
                    binding => binding,
                },
                other => evaluate_expr_with_lookup_diagnostic(other, &env, diagnostics, label),
            }?;
            let name = match value {
                Value::Str(name) => name,
                other => format!("{other:?}"),
            };
            if let Some(role) = Role::from_name(&name).filter(|role| role.applies_to(ty)) {
                return Some((role, name));
            }
            let choices = ROLES
                .iter()
                .filter_map(|(role, name)| role.applies_to(ty).then_some(*name))
                .collect::<Vec<_>>()
                .join(", ");
            let message = if choices.is_empty() {
                format!("'{label}': {ty} does not accept visual roles.")
            } else {
                format!("'{label}': role '{name}' is invalid for {ty}. Choose {choices}.")
            };
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    message,
                )
                .with_subject(label)
                .with_byte_span(prop.value_span),
            );
            None
        });

        let mut defaults = Vec::new();
        if !matches!(self.style_preset, StylePreset::Default) {
            self.add_style_defaults(
                label,
                ty,
                props,
                selection.as_ref().map(|(role, _)| *role),
                &mut defaults,
            );
        }
        if defaults.is_empty() && role_prop.is_none() {
            return Cow::Borrowed(props);
        }
        defaults.extend(
            props.iter().filter(|prop| role_prop.is_none() || prop.name != "role").cloned(),
        );
        if let Some((_, name)) = selection {
            defaults.push(Property::new("role", Expr::Str(name)));
        }
        Cow::Owned(defaults)
    }

    fn add_style_defaults(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        role: Option<Role>,
        defaults: &mut Vec<Property>,
    ) {
        let scale = self.style_scale();
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
                add("stroke", token(("text", "muted")));
            }
        } else if ty == "PlotCurve" {
            let (width, color) = if role == Some(Role::Reference) {
                (2.5, ("text", "secondary"))
            } else {
                (
                    if matches!(self.style_preset, StylePreset::Compact) {
                        4.0
                    } else {
                        5.0
                    },
                    ("accent", "primary"),
                )
            };
            add("stroke_width", Expr::Num(width * scale));
            if !has_authored_stroke(props) {
                add("stroke", token(color));
            }
        } else if ROLE_ACTORS.contains(&ty) {
            let mut typography = self.style_preset.typography(role.unwrap_or(if ty == "Counter" {
                Role::Metric
            } else {
                Role::Body
            }));
            if role.is_none() && matches!(ty, "Typst" | "Math") {
                typography.size = 42.0;
            }
            add("font_size", Expr::Num(typography.size * scale));
            add("color", token(typography.ink));
            if let Some(weight) = typography.weight {
                add("font_weight", Expr::Num(weight));
            }
            if matches!(self.style_preset, StylePreset::Editorial | StylePreset::Compact) {
                add(
                    "line_height",
                    Expr::Num(if role == Some(Role::Title) {
                        1.04
                    } else {
                        1.16
                    }),
                );
            }
        }
    }
}

fn token((section, name): (&str, &str)) -> Expr {
    Expr::Path(vec![section.into(), name.into()])
}

fn has_authored_stroke(props: &[Property]) -> bool {
    props.iter().any(|prop| {
        matches!(prop.name.as_str(), "color" | "stroke" | "stroke_color" | "stroke_gradient")
    })
}
