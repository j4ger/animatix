//! Property resolution: extract actor properties, warn about dropped declaration
//! properties, map math coordinates to screen pixels for graph children.

use super::*;
use crate::ast::Property;

/// The plot-family declaration keys both property walkers parse identically.
///
/// `extract_actor_properties` feeds the generic actor path (graph hosts,
/// plot curves) and `parse_plot_props` feeds the plot-family builders. Keeping
/// one parser for the shared keys stops their defaults, value handling and
/// diagnostics from drifting apart; each caller then parses only the keys that
/// are specific to it.
pub(super) struct PlotPropBag {
    pub initial_size: [f32; 2],
    pub x_domain: [f64; 2],
    pub y_domain: [f64; 2],
    pub t_domain: [f64; 2],
    pub func: Option<(
        Vec<String>,
        Box<crate::timeline::modifier_runtime::ir::CompiledExpr>,
        CapturedEnv,
    )>,
    pub tolerance: f64,
    pub max_depth: f64,
    pub resolution: f64,
    /// `None` leaves the caller's default in place.
    pub kind: Option<PlotCurveKind>,
}

impl Default for PlotPropBag {
    fn default() -> Self {
        Self {
            initial_size: DEFAULT_LAYOUT_HALF_SIZE,
            x_domain: [-10.0, 10.0],
            y_domain: [-10.0, 10.0],
            t_domain: [0.0, std::f64::consts::TAU],
            func: None,
            tolerance: 0.5,
            max_depth: 10.0,
            resolution: 96.0,
            kind: None,
        }
    }
}

impl PlotPropBag {
    /// Whether [`Self::parse_prop`] consumes this declaration key.
    pub(super) fn handles(name: &str) -> bool {
        matches!(
            name,
            "size"
                | "radius"
                | "x_domain"
                | "y_domain"
                | "t_domain"
                | "func"
                | "tolerance"
                | "max_depth"
                | "resolution"
                | "kind"
        )
    }

    /// Parse every shared plot key from `props`, in declaration order.
    pub(super) fn from_props(
        props: &[Property],
        label: &str,
        eval_env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let mut bag = Self::default();
        for prop in props {
            bag.parse_prop(prop, label, eval_env, diagnostics);
        }
        bag
    }

    fn parse_prop(
        &mut self,
        prop: &Property,
        label: &str,
        eval_env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let prop_subject = format!("{}.{}", label, prop.name);
        match prop.name.as_str() {
            "size" => {
                // A non-numeric size spec (`percent`, `auto`, `fill`, …) is
                // applied by the layout path; the numeric pair only seeds the
                // half-size both plot layouts and bounds use.
                let size_val = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .unwrap_or(Value::Num(0.0));
                if let Value::Vec2([w, h]) = size_val {
                    self.initial_size[0] = w as f32 / 2.0;
                    self.initial_size[1] = h as f32 / 2.0;
                }
            },
            "radius" => {
                let v = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .unwrap_or(Value::Num(0.0));
                let r = v.as_num() as f32;
                self.initial_size = [r, r];
            },
            "x_domain" => match evaluate_expr_with_lookup_diagnostic(
                &prop.value,
                eval_env,
                diagnostics,
                &prop_subject,
            ) {
                Some(Value::Vec2([min, max])) => self.x_domain = [min, max],
                Some(v) => tracing::warn!(
                    "{}: 'x_domain' expects a (min, max) tuple, got {:?}",
                    prop_subject,
                    v
                ),
                None => {}, // eval error already reported as a diagnostic
            },
            "y_domain" => match evaluate_expr_with_lookup_diagnostic(
                &prop.value,
                eval_env,
                diagnostics,
                &prop_subject,
            ) {
                Some(Value::Vec2([min, max])) => self.y_domain = [min, max],
                Some(v) => tracing::warn!(
                    "{}: 'y_domain' expects a (min, max) tuple, got {:?}",
                    prop_subject,
                    v
                ),
                None => {}, // eval error already reported as a diagnostic
            },
            "t_domain" => match evaluate_expr_with_lookup_diagnostic(
                &prop.value,
                eval_env,
                diagnostics,
                &prop_subject,
            ) {
                Some(Value::Vec2([min, max])) => self.t_domain = [min, max],
                Some(v) => tracing::warn!(
                    "{}: 't_domain' expects a (min, max) tuple, got {:?}",
                    prop_subject,
                    v
                ),
                None => {}, // eval error already reported as a diagnostic
            },
            "func" => match evaluate_expr_with_lookup_diagnostic(
                &prop.value,
                eval_env,
                diagnostics,
                &prop_subject,
            ) {
                Some(Value::Closure(args, body, captures)) => {
                    self.func = Some((args, body, captures))
                },
                Some(v) => {
                    tracing::warn!("{}: 'func' expects a closure, got {:?}", prop_subject, v)
                },
                None => {}, // eval error already reported as a diagnostic
            },
            "tolerance" => {
                let v = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .unwrap_or(Value::Num(0.0));
                self.tolerance = v.as_num();
            },
            "max_depth" => {
                let v = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .unwrap_or(Value::Num(0.0));
                self.max_depth = v.as_num();
            },
            "resolution" => {
                let v = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                )
                .unwrap_or(Value::Num(96.0));
                self.resolution = v.as_num();
            },
            "kind" => {
                if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    eval_env,
                    diagnostics,
                    &prop_subject,
                ) {
                    if let Some(kind) = PlotCurveKind::from_str(&v.as_str().to_lowercase()) {
                        self.kind = Some(kind);
                    } else {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!("Invalid plot kind: '{}'", v.as_str()),
                            )
                            .with_subject(&prop_subject),
                        );
                    }
                }
            },
            _ => {}, // not a shared plot key; the caller's own loop handles it
        }
    }
}

impl Timeline {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn extract_actor_properties(
        &self,
        label: &str,
        _ty: &str,
        props: &[Property],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> ExtractedActorProperties {
        let initial_eval_env = self.build_eval_env(time_ms as u64);
        let PlotPropBag {
            initial_size,
            x_domain,
            y_domain,
            t_domain,
            func,
            tolerance,
            max_depth,
            resolution,
            kind,
        } = PlotPropBag::from_props(props, label, &initial_eval_env, diagnostics);
        let mut at_expr: Option<Expr> = None;
        let mut anchor_expr: Option<Expr> = None;
        let mut offset_expr: Option<Expr> = None;
        let mut size_decl: Option<ParsedSizeDecl> = None;

        for prop in props {
            match prop.name.as_str() {
                "at" => at_expr = Some(prop.value.clone()),
                "anchor" => anchor_expr = Some(prop.value.clone()),
                "offset" => offset_expr = Some(prop.value.clone()),
                // Layout spec and numeric half-size come from the same
                // expression; parse the spec here so the declaration tail only
                // consumes the prepared value. Repeated keys keep the first
                // declaration, like the old single-pass apply loop.
                "size" if size_decl.is_none() => {
                    size_decl = Some(ParsedSizeDecl {
                        spec: crate::timeline::taffy_layout::parse_size_spec(&prop.value),
                        value: prop.value.clone(),
                    });
                },
                _ => {}, // shared plot keys and the generic pipeline handle the rest
            }
        }

        ExtractedActorProperties {
            initial_size,
            size_decl,
            x_domain,
            y_domain,
            t_domain,
            func,
            tolerance,
            max_depth,
            resolution,
            kind,
            at_expr,
            anchor_expr,
            offset_expr,
            graph_padding: {
                // Parse graph padding directly from props so it's available before
                // process_plot_actor runs (which stores it in env).
                let mut gp = [0.0f64; 4];
                for prop in props {
                    if prop.name == "padding" {
                        if let Some(v) = evaluate_expr_with_lookup_diagnostic(
                            &prop.value,
                            &initial_eval_env,
                            diagnostics,
                            &format!("{}.padding", label),
                        ) {
                            gp = match v {
                                Value::Vec4([l, r, t, b]) => [l, r, t, b],
                                Value::Num(n) => [n, n, n, n],
                                _ => [0.0; 4],
                            };
                        }
                        break;
                    }
                }
                gp
            },
            x_scale: props
                .iter()
                .find(|p| p.name == "x_scale")
                .and_then(|p| {
                    evaluate_expr_with_lookup_diagnostic(
                        &p.value,
                        &initial_eval_env,
                        diagnostics,
                        &format!("{}.x_scale", label),
                    )
                })
                .map(|v| super::utils::ScaleType::from_str(&v.as_str()))
                .unwrap_or(super::utils::ScaleType::Linear),
            y_scale: props
                .iter()
                .find(|p| p.name == "y_scale")
                .and_then(|p| {
                    evaluate_expr_with_lookup_diagnostic(
                        &p.value,
                        &initial_eval_env,
                        diagnostics,
                        &format!("{}.y_scale", label),
                    )
                })
                .map(|v| super::utils::ScaleType::from_str(&v.as_str()))
                .unwrap_or(super::utils::ScaleType::Linear),
        }
    }

    /// When an actor is declared inside a Graph, map its position properties
    /// from math coordinates to screen pixels based on the parent's domain and size.
    pub(super) fn map_props_to_graph_parent(
        &self,
        parent_label: &str,
        props: &[Property],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Property> {
        let eval_env = self.build_eval_env(time_ms as u64);

        // The host's coordinate frame component is the only source for the
        // domain, padding and scales used to map child positions.
        let mut x_domain = [-10.0, 10.0];
        let mut y_domain = [-10.0, 10.0];
        let mut p_padding = [0.0; 4];
        let mut p_x_scale = super::utils::ScaleType::Linear;
        let mut p_y_scale = super::utils::ScaleType::Linear;

        if let Some(frame) = self.coordinate_frame_for(parent_label) {
            x_domain = frame.x_domain;
            y_domain = frame.y_domain;
            p_padding = frame.padding;
            p_x_scale = frame.x_scale;
            p_y_scale = frame.y_scale;
        }

        let p_size = self
            .env
            .get(&format!("{}_size", parent_label))
            .or_else(|| {
                self.env
                    .get(crate::timeline::env_keys::side_channel(parent_label, "size").as_str())
            })
            .and_then(|v| match v {
                Value::Vec2(s) => Some(s),
                _ => None,
            })
            .unwrap_or([500.0, 500.0]);
        let parent_pos = self
            .world
            .get_id(parent_label)
            .map(|id| self.world.geometry(id).position.last([0.0, 0.0]))
            .unwrap_or([0.0, 0.0]);

        props
            .iter()
            .map(|prop| {
                let needs_mapping = prop.name == "at"
                    || prop.name == "position"
                    || prop.name == "from"
                    || prop.name == "to";
                if !needs_mapping {
                    return prop.clone();
                }

                let val = evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    &eval_env,
                    diagnostics,
                    &format!("{}.{}", parent_label, prop.name),
                );
                let (mx, my) = match val {
                    Some(Value::Vec2([x, y])) => (x, y),
                    _ => return prop.clone(),
                };

                let ctx = super::utils::GraphContext::new(
                    super::utils::GraphScaleConfig::new(x_domain, y_domain, p_x_scale, p_y_scale),
                    super::utils::GraphGeometry::new(
                        p_size,
                        [parent_pos[0] as f64, parent_pos[1] as f64],
                        p_padding,
                    ),
                );
                let [screen_x, screen_y] = super::utils::graph_math_to_screen(
                    mx, my, &ctx.scale, &ctx.geo,
                    false, // absolute coordinates for actor properties
                );

                Property::new(
                    &prop.name,
                    Expr::Tuple(vec![Expr::Num(screen_x), Expr::Num(screen_y)]),
                )
            })
            .collect()
    }
    /// Warn about declaration properties no build path consumes.
    ///
    /// A name is known when the property registry binds it, the actor type's
    /// primitive declares it, or an extension registry declares it for the
    /// type; everything else is dropped silently by every consumer, which is
    /// how a typo'd property (`colour:`) ships as a scene that "works" without
    /// the value. The known sets are the single-source tables, so this warning
    /// cannot drift from what the build actually reads.
    ///
    /// A property name the registry knows, but which this actor's primitive
    /// never reads, is the sibling check below.
    pub(crate) fn warn_unknown_declaration_properties(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let declared = self
            .primitive_registry
            .find(ty)
            .map(|primitive| primitive.declared_property_names());
        for prop in props {
            if crate::timeline::property_registry::lookup_property(&prop.name).is_some()
                || declared
                    .as_ref()
                    .is_some_and(|names| names.iter().any(|name| *name == prop.name))
                || self
                    .extensions
                    .as_ref()
                    .is_some_and(|registry| registry.property_spec(ty, &prop.name).is_some())
            {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownProperty,
                    DiagnosticPhase::Build,
                    format!(
                        "Actor '{label}' ({ty}) declares '{}' which no build path consumes; \
                         the property is dropped. Check the spelling against the property \
                         registry or the primitive's declared properties.",
                        prop.name
                    ),
                )
                .with_subject(format!("{label}.{}", prop.name)),
            );
        }
        // Same call sites, same coverage: a name that exists but is ignored by
        // this primitive is the sibling of a name that does not exist at all.
        self.warn_inapplicable_declaration_properties(label, ty, props, diagnostics);
    }

    /// Warn about a property name the registry knows, but which this actor's
    /// primitive never reads.
    ///
    /// `unknown-property` above catches a typo; this catches the worse case,
    /// because a misspelling is at least visibly wrong while this one is
    /// indistinguishable from correct source — `stroke_width: 6` on a `Text`
    /// names a real property that `Text` ignores, and the value disappears with
    /// no diagnostic anywhere. It is the same class as the plot-family `opacity`
    /// and `Arrow` `color` drops, found by eye this round.
    ///
    /// The predicate is `Applicable::includes`, the table the inspector already
    /// filters its property list with, so the editor and the build cannot
    /// disagree about what a type takes. Two honest limits:
    ///
    /// - Rows declared `Applicable::Everything` (`color`, `opacity`, `at`, …) pass unconditionally,
    ///   so a primitive that ignores one of those is invisible here. That is a defect in the table,
    ///   not in this check, and the fix is to narrow the row.
    /// - Coverage matches `warn_unknown_declaration_properties`' exactly, which means the
    ///   plot-family dispatch is not covered at all (it never calls either function). Its runtime
    ///   parameters are the reason: `freq: 2` on `func: (x) => sin(freq * x)` is read by the
    ///   author's closure, not by a primitive, and the exemption would have to be
    ///   `plot_runtime_params`.
    pub(super) fn warn_inapplicable_declaration_properties(
        &self,
        label: &str,
        ty: &str,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(caps) = animatix_std::caps_for_type(ty) else {
            // Extension primitives have no catalog caps; their property surface
            // is the extension registry, which `warn_unknown_declaration_
            // properties` already consults.
            return;
        };
        for prop in props {
            let Some(descriptor) = animatix_core::property::descriptor(&prop.name) else {
                continue; // an unknown name is the other warning's subject
            };
            if descriptor.applicable.includes(&caps, ty) {
                continue;
            }
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InapplicableProperty,
                    DiagnosticPhase::Build,
                    format!(
                        "Actor '{label}' ({ty}) declares '{}' but {ty} never reads it, so the \
                         value is dropped. Check the primitive's property list, or move the \
                         value to a property {ty} consumes.",
                        prop.name
                    ),
                )
                .with_subject(format!("{label}.{}", prop.name))
                .with_byte_span(prop.value_span),
            );
        }
    }
}
