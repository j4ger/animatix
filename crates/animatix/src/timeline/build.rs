use super::*;

impl Timeline {
    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_diagnostics(ast).output
    }

    pub fn build_with_diagnostics(ast: &[Stmt]) -> BuildReport<Self> {
        let mut timeline = Self::new();
        load_standard_library(&mut timeline.env);
        timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
        let mut current_build_time_ms = 0.0;
        let mut diagnostics = Vec::new();

        for stmt in ast {
            if let Stmt::Config { settings } = stmt {
                timeline.apply_config_settings(settings, &mut diagnostics);
            }
        }

        for stmt in ast {
            match stmt {
                Stmt::Config { .. } => {}
                Stmt::Keyframe { time, body } => {
                    current_build_time_ms = time_to_ms(time);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                }
                Stmt::RelativeKeyframe { offset, body } => {
                    current_build_time_ms += time_to_ms(offset);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                }
                Stmt::ActorDecl { .. }
                | Stmt::Assignment { .. }
                | Stmt::Sequence { .. }
                | Stmt::Stagger { .. } => {
                    timeline.process_body(
                        current_build_time_ms,
                        &[stmt.clone()],
                        None,
                        &mut diagnostics,
                    );
                }
                _ => {}
            }
        }
        BuildReport::new(timeline, diagnostics)
    }

    fn apply_colorscheme(&mut self, colorscheme: ResolvedColorscheme) {
        colorscheme.seed_environment(&mut self.env);
        let background = colorscheme
            .color("scene.background")
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let mut bg_track = PropertyTrack::new(background);
        bg_track.add_keyframe(0, background, Easing::Linear);
        self.background_color = bg_track;
        self.colorscheme = colorscheme;
    }

    fn apply_config_settings(
        &mut self,
        settings: &[crate::ast::Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for setting in settings {
            if setting.name != "colorscheme" {
                continue;
            }

            let Some(raw_name) = config_string_value(&setting.value) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidConfigValue,
                        DiagnosticPhase::Build,
                        "Config key 'colorscheme' expects a built-in scheme name string such as \"editorial-dark\".".to_string(),
                    )
                    .with_subject("colorscheme"),
                );
                continue;
            };

            let Some(built_in) = BuiltInColorscheme::from_name(&raw_name) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownColorscheme,
                        DiagnosticPhase::Build,
                        format!(
                            "Unknown colorscheme '{raw_name}'; using the default-dark built-in scheme instead."
                        ),
                    )
                    .with_subject("colorscheme"),
                );
                continue;
            };

            self.apply_colorscheme(built_in.resolved());
        }
    }

    pub(super) fn auto_color_for_label(&mut self, label: &str) -> Option<[f32; 4]> {
        if self.colorscheme.auto_cycle.is_empty() {
            return None;
        }

        let slot = if let Some(slot) = self.auto_color_assignments.get(label) {
            *slot
        } else {
            let slot = self.next_auto_color_index;
            self.auto_color_assignments.insert(label.to_string(), slot);
            self.next_auto_color_index += 1;
            slot
        };

        Some(self.colorscheme.auto_cycle[slot % self.colorscheme.auto_cycle.len()])
    }

    pub(super) fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        if !self.nodes.contains_key(&label) {
            self.nodes.insert(
                label.clone(),
                SceneNode {
                    label: label.clone(),
                    children: Vec::new(),
                },
            );
            if let Some(parent) = parent_label {
                if let Some(p) = self.nodes.get_mut(parent) {
                    if !p.children.contains(&label) {
                        p.children.push(label.clone());
                    }
                }
            } else {
                if !self.root_nodes.contains(&label) {
                    self.root_nodes.push(label.clone());
                }
            }
        }
    }

    /// Apply layout algorithm for Row and Col containers.
    /// Computes and sets child positions based on container type, gap, and alignment.
    ///
    /// - `gap`: spacing between children (default 0.0)
    /// - `align`: alignment perpendicular to the layout axis.
    ///   For Row: "center" (default), "start" (top), "end" (bottom)
    ///   For Col: "center" (default), "start" (left), "end" (right)
    fn process_inline_items(
        &mut self,
        time_ms: f64,
        items: &[crate::ast::InlineItem],
        parent_label: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for item in items {
            match item {
                crate::ast::InlineItem::Anonymous {
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    let id = format!("__anon_{}", self.anon_counter);
                    self.anon_counter += 1;
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        label: id.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                }
                crate::ast::InlineItem::Labeled {
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        label: label.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                }
            }
        }
    }

    pub(super) fn process_body(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for stmt in body {
            match stmt {
                Stmt::Text { .. } => {
                    self.process_text_like_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::Math { .. } => {
                    self.process_text_like_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::Code { .. } => {
                    self.process_text_like_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::Svg { .. } => {
                    self.process_media_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::Image { .. } => {
                    self.process_media_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::ActorDecl {
                    is_pub: _,
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                } => {
                    self.add_node(label.clone(), parent_label);
                    let primitive = PrimitiveDescriptor::for_actor_type(ty);

                    let mut x_domain = [-10.0, 10.0];
                    let mut y_domain = [-10.0, 10.0];
                    let mut t_domain = [0.0, std::f64::consts::TAU];
                    let mut func = None;
                    let mut initial_size = [50.0, 50.0];
                    let mut tolerance = 0.5;
                    let mut max_depth = 10.0;
                    let mut resolution = 96.0;
                    let mut at_expr: Option<Expr> = None;
                    let mut anchor_expr: Option<Expr> = None;
                    let mut offset_expr: Option<Expr> = None;
                    let initial_eval_env = self.build_eval_env(time_ms as u64);

                    for prop in props {
                        let prop_subject = format!("{}.{}", label, prop.name);
                        match prop.name.as_str() {
                            "size" => {
                                let size_val = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    initial_size[0] = w as f32 / 2.0;
                                    initial_size[1] = h as f32 / 2.0;
                                }
                            }
                            "radius" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                initial_size = [r, r];
                            }
                            "x_domain" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    x_domain = [min, max];
                                }
                            }
                            "y_domain" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    y_domain = [min, max];
                                }
                            }
                            "t_domain" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([min, max]) = v {
                                    t_domain = [min, max];
                                }
                            }
                            "func" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Closure(args, body) = v {
                                    func = Some((args, body));
                                }
                            }
                            "tolerance" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                tolerance = v.as_num();
                            }
                            "max_depth" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                max_depth = v.as_num();
                            }
                            "resolution" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &initial_eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(96.0));
                                resolution = v.as_num();
                            }
                            "at" => at_expr = Some(prop.value.clone()),
                            "anchor" => anchor_expr = Some(prop.value.clone()),
                            "offset" => offset_expr = Some(prop.value.clone()),
                            _ => {}
                        }
                    }

                    if primitive.is_graph_host() {
                        self.env
                            .set(&format!("{}_x_domain", label), Value::Vec2(x_domain));
                        self.env
                            .set(&format!("{}_y_domain", label), Value::Vec2(y_domain));
                        self.env.set(
                            &format!("{}_size", label),
                            Value::Vec2([
                                initial_size[0] as f64 * 2.0,
                                initial_size[1] as f64 * 2.0,
                            ]),
                        );
                    }

                    self.process_inline_items(time_ms, children, label, diagnostics);
                    let eval_env = self.build_eval_env(time_ms as u64);
                    let existing_track = self
                        .tracks
                        .get(label)
                        .cloned()
                        .unwrap_or_else(|| AnimationTrack::new(label.clone()));

                    let mut position = existing_track.position.last_value();
                    let mut size = existing_track.size.last_value();
                    let mut line_from = existing_track.line_from.last_value();
                    let mut line_to = existing_track.line_to.last_value();
                    let mut arc_angles = existing_track.arc_angles.last_value();
                    let mut color = existing_track.color.last_value();
                    let vector_shape = vector_shape_primitive_for_actor_type(ty);
                    let shape_type = shape_type_for_actor(ty);
                    let opacity = existing_track.opacity.last_value();
                    let mut stroke_width = existing_track.stroke_width.last_value();
                    let mut stroke_color = existing_track.stroke_color.last_value();
                    let mut stroke_progress = existing_track.stroke_progress.last_value();
                    let mut fill_opacity = existing_track.fill_opacity.last_value();
                    let mut gap = 0.0f32;
                    let mut align: Option<String> = None;
                    let mut cols: Option<usize> = None;
                    let mut vector_shape_state = VectorShapeState::new(
                        size,
                        line_from,
                        line_to,
                        arc_angles,
                    );
                    apply_vector_shape_defaults(ty, &mut vector_shape_state);
                    size = vector_shape_state.size;

                    let ParsedTimingModifiers {
                        duration_ms,
                        delay_ms,
                        easing,
                        morph_options,
                    } = parse_timing_modifiers(
                        modifiers,
                        ModifierHost::ActorDeclaration,
                        Some(label),
                        diagnostics,
                    );
                    let t_start_ms = (time_ms + delay_ms) as u64;
                    let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
                    let supports_morph_options =
                        !existing_track.vector_paths.keyframes.is_empty() && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            "Morph-specific modifiers on actor declarations require a path-morphing re-declaration with non-zero duration; ignoring them for now.".to_string(),
                            Some(label),
                        );
                    }

                    for prop in props {
                        let prop_subject = format!("{}.{}", label, prop.name);
                        match prop.name.as_str() {
                            "at" | "anchor" | "offset" => {}
                            "radius" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                size = [r, r];
                                vector_shape_state.size = size;
                                vector_shape_state.regular_polygon_radius = r;
                            }
                            "size" => {
                                let size_val = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                if let Value::Vec2([w, h]) = size_val {
                                    size[0] = w as f32 / 2.0;
                                    size[1] = h as f32 / 2.0;
                                    vector_shape_state.size = size;
                                }
                            }
                            "color" => {
                                if matches!(&prop.value, Expr::Ident(name) if name == "auto") {
                                    if let Some(actor_color) = self.auto_color_for_label(label) {
                                        color = actor_color;
                                        if primitive.is_plot_curve() {
                                            stroke_color = actor_color;
                                        }
                                    } else {
                                        diagnostics.push(
                                            Diagnostic::warning(
                                                DiagnosticCode::UnknownColorReference,
                                                DiagnosticPhase::Build,
                                                format!(
                                                    "Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.",
                                                    label
                                                ),
                                            )
                                            .with_subject(&prop_subject),
                                        );
                                    }
                                } else if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label,
                                        "color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                {
                                    color = resolved_color;
                                    if primitive.is_plot_curve() {
                                        stroke_color = resolved_color;
                                    }
                                }
                            }
                            "stroke_width" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "width" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "stroke_color" => {
                                if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label,
                                        "stroke_color",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                {
                                    stroke_color = resolved_color;
                                }
                            }
                            "stroke" => {
                                if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label,
                                        "stroke",
                                        &prop.value,
                                        &eval_env,
                                        diagnostics,
                                        &prop_subject,
                                    )
                                {
                                    stroke_color = resolved_color;
                                }
                            }
                            "stroke_progress" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                stroke_progress = v.as_num() as f32;
                            }
                            "fill_opacity" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                fill_opacity = v.as_num() as f32;
                            }
                            "gap" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(0.0));
                                gap = v.as_num() as f32;
                            }
                            "align" => {
                                if let Expr::Str(s) = &prop.value {
                                    align = Some(s.clone());
                                } else if let Expr::Ident(s) = &prop.value {
                                    align = Some(s.clone());
                                }
                            }
                            "cols" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                )
                                .unwrap_or(Value::Num(1.0));
                                cols = Some(v.as_num().max(1.0) as usize);
                            }
                            _ if vector_shape.is_some() => {
                                if apply_vector_shape_property(
                                    ty,
                                    &prop.name,
                                    &prop.value,
                                    &eval_env,
                                    diagnostics,
                                    &prop_subject,
                                    &mut vector_shape_state,
                                ) {
                                    size = vector_shape_state.size;
                                    line_from = vector_shape_state.line_from;
                                    line_to = vector_shape_state.line_to;
                                    arc_angles = vector_shape_state.arc_angles;
                                }
                            }
                            _ => {}
                        }
                    }

                    if vector_shape.is_some() {
                        finalize_vector_shape_state(ty, &mut vector_shape_state);
                        size = vector_shape_state.size;
                        line_from = vector_shape_state.line_from;
                        line_to = vector_shape_state.line_to;
                        arc_angles = vector_shape_state.arc_angles;
                    }

                    // For Graph types, make them invisible (container only)
                    if primitive.is_graph_host() {
                        fill_opacity = 0.0;
                        stroke_width = 0.0;
                    }

                    let track = self
                        .tracks
                        .entry(label.clone())
                        .or_insert_with(|| AnimationTrack::new(label.clone()));

                    if let Some((binding, bound_position)) =
                        resolve_position_binding_with_lookup_diagnostic(
                            at_expr.as_ref(),
                            anchor_expr.as_ref(),
                            offset_expr.as_ref(),
                            &eval_env,
                            diagnostics,
                            label,
                        )
                    {
                        preserve_discrete_position_state_before(track, t_start_ms);
                        set_track_position_binding(track, t_start_ms, binding);
                        if let Some(bound_position) = bound_position {
                            position = bound_position;
                            mark_track_manual_position(track, t_start_ms);
                        } else {
                            mark_track_manual_position(track, t_start_ms);
                        }
                    } else if primitive.is_layout_container() && parent_label.is_none() {
                        preserve_discrete_position_state_before(track, t_start_ms);
                        set_track_position_binding(
                            track,
                            t_start_ms,
                            PositionBinding::ContainerDefault {
                                anchor: SceneAnchor::Center,
                            },
                        );
                    }

                    let mut vello_paths = vec![];

                    if primitive.is_graph_host() {
                        let mut path = kurbo::BezPath::new();
                        // X axis
                        let x_axis_y = if y_domain[0] <= 0.0 && y_domain[1] >= 0.0 {
                            size[1] as f64
                                * (1.0 - 2.0 * (0.0 - y_domain[0]) / (y_domain[1] - y_domain[0]))
                        } else {
                            size[1] as f64
                        };
                        path.move_to((-(size[0] as f64), x_axis_y));
                        path.line_to((size[0] as f64, x_axis_y));

                        // Y axis
                        let y_axis_x = if x_domain[0] <= 0.0 && x_domain[1] >= 0.0 {
                            size[0] as f64
                                * (-1.0 + 2.0 * (0.0 - x_domain[0]) / (x_domain[1] - x_domain[0]))
                        } else {
                            -(size[0] as f64)
                        };
                        path.move_to((y_axis_x, -(size[1] as f64)));
                        path.line_to((y_axis_x, size[1] as f64));

                        vello_paths.push(crate::timeline::vello_path::VelloPath {
                            path,
                            fill: None,
                            stroke: Some((
                                vello::peniko::Color::from_rgba8(255, 255, 255, 255),
                                2.0,
                            )),
                        });
                    } else if primitive.is_plot_curve() {
                        let p_label = parent_label.unwrap_or("").to_string();
                        let mut p_x_domain = [-10.0, 10.0];
                        let mut p_y_domain = [-10.0, 10.0];
                        let mut p_size = [500.0, 500.0];

                        if let Some(Value::Vec2(xd)) =
                            self.env.get(&format!("{}_x_domain", p_label))
                        {
                            p_x_domain = xd;
                        }
                        if let Some(Value::Vec2(yd)) =
                            self.env.get(&format!("{}_y_domain", p_label))
                        {
                            p_y_domain = yd;
                        }
                        if let Some(Value::Vec2(sz)) = self.env.get(&format!("{}_size", p_label)) {
                            p_size = sz;
                        }

                        if let Some((args, body)) = func {
                            let mut env_copy = eval_env.clone();
                            let arg_name = if !args.is_empty() {
                                args[0].clone()
                            } else {
                                "x".to_string()
                            };

                            let (min_t, max_t) = if ty == "CartesianPlot" {
                                (p_x_domain[0], p_x_domain[1])
                            } else if ty == "ImplicitPlot" {
                                (0.0, 0.0)
                            } else {
                                (t_domain[0], t_domain[1])
                            };

                            if ty == "ImplicitPlot" {
                                let path = build_implicit_plot_path(
                                    &mut env_copy,
                                    &args,
                                    &body,
                                    &p_x_domain,
                                    &p_y_domain,
                                    &p_size,
                                    resolution.round().max(8.0) as usize,
                                );
                                vello_paths.push(crate::timeline::vello_path::VelloPath {
                                    path,
                                    fill: None,
                                    stroke: if stroke_width > 0.0 {
                                        Some((
                                            vello::peniko::Color::from_rgba8(
                                                (stroke_color[0] * 255.0) as u8,
                                                (stroke_color[1] * 255.0) as u8,
                                                (stroke_color[2] * 255.0) as u8,
                                                (stroke_color[3] * 255.0) as u8,
                                            ),
                                            stroke_width,
                                        ))
                                    } else {
                                        None
                                    },
                                });
                            } else {
                                env_copy.set(&arg_name, Value::Num(min_t));
                                let start_eval =
                                    evaluate_expr(&body, &env_copy).unwrap_or(Value::Num(0.0));
                                let (start_math_x, start_math_y) = if ty == "CartesianPlot" {
                                    (min_t, start_eval.as_num())
                                } else if ty == "ParametricPlot" {
                                    match start_eval {
                                        Value::Vec2([x, y]) => (x, y),
                                        _ => (0.0, 0.0),
                                    }
                                } else {
                                    let start_val = start_eval.as_num();
                                    (start_val * min_t.cos(), start_val * min_t.sin())
                                };
                                let start_screen_x = -(p_size[0] / 2.0)
                                    + p_size[0]
                                        * ((start_math_x - p_x_domain[0])
                                            / (p_x_domain[1] - p_x_domain[0]));
                                let start_screen_y = (p_size[1] / 2.0)
                                    - p_size[1]
                                        * ((start_math_y - p_y_domain[0])
                                            / (p_y_domain[1] - p_y_domain[0]));

                                env_copy.set(&arg_name, Value::Num(max_t));
                                let end_eval =
                                    evaluate_expr(&body, &env_copy).unwrap_or(Value::Num(0.0));
                                let (end_math_x, end_math_y) = if ty == "CartesianPlot" {
                                    (max_t, end_eval.as_num())
                                } else if ty == "ParametricPlot" {
                                    match end_eval {
                                        Value::Vec2([x, y]) => (x, y),
                                        _ => (0.0, 0.0),
                                    }
                                } else {
                                    let end_val = end_eval.as_num();
                                    (end_val * max_t.cos(), end_val * max_t.sin())
                                };
                                let end_screen_x = -(p_size[0] / 2.0)
                                    + p_size[0]
                                        * ((end_math_x - p_x_domain[0])
                                            / (p_x_domain[1] - p_x_domain[0]));
                                let end_screen_y = (p_size[1] / 2.0)
                                    - p_size[1]
                                        * ((end_math_y - p_y_domain[0])
                                            / (p_y_domain[1] - p_y_domain[0]));

                                let p0 = kurbo::Point::new(start_screen_x, start_screen_y);
                                let p1 = kurbo::Point::new(end_screen_x, end_screen_y);

                                let mut pts = vec![p0];

                                if ty == "CartesianPlot" {
                                    sample_recursive_cartesian(
                                        min_t,
                                        max_t,
                                        p0,
                                        p1,
                                        0,
                                        max_depth as usize,
                                        tolerance,
                                        &mut env_copy,
                                        &arg_name,
                                        &body,
                                        &p_x_domain,
                                        &p_y_domain,
                                        &p_size,
                                        &mut pts,
                                    );
                                } else if ty == "PolarPlot" {
                                    sample_recursive_polar(
                                        min_t,
                                        max_t,
                                        p0,
                                        p1,
                                        0,
                                        max_depth as usize,
                                        tolerance,
                                        &mut env_copy,
                                        &arg_name,
                                        &body,
                                        &p_x_domain,
                                        &p_y_domain,
                                        &p_size,
                                        &mut pts,
                                    );
                                } else {
                                    sample_recursive_parametric(
                                        min_t,
                                        max_t,
                                        p0,
                                        p1,
                                        0,
                                        max_depth as usize,
                                        tolerance,
                                        &mut env_copy,
                                        &arg_name,
                                        &body,
                                        &p_x_domain,
                                        &p_y_domain,
                                        &p_size,
                                        &mut pts,
                                    );
                                }

                                let mut path = kurbo::BezPath::new();
                                let mut first = true;
                                for pt in pts {
                                    if pt.x.is_nan() || pt.y.is_nan() {
                                        first = true;
                                    } else if first {
                                        path.move_to((pt.x, pt.y));
                                        first = false;
                                    } else {
                                        path.line_to((pt.x, pt.y));
                                    }
                                }
                                vello_paths.push(crate::timeline::vello_path::VelloPath {
                                    path,
                                    fill: None,
                                    stroke: if stroke_width > 0.0 {
                                        Some((
                                            vello::peniko::Color::from_rgba8(
                                                (stroke_color[0] * 255.0) as u8,
                                                (stroke_color[1] * 255.0) as u8,
                                                (stroke_color[2] * 255.0) as u8,
                                                (stroke_color[3] * 255.0) as u8,
                                            ),
                                            stroke_width,
                                        ))
                                    } else {
                                        None
                                    },
                                });
                            }
                        }
                    } else if !primitive.is_plot() {
                        vector_shape_state.size = size;
                        vector_shape_state.line_from = line_from;
                        vector_shape_state.line_to = line_to;
                        vector_shape_state.arc_angles = arc_angles;
                        let vello_path = build_vector_shape_vello_path(
                            shape_type,
                            &vector_shape_state,
                            VectorShapeStyle {
                                color,
                                stroke_width,
                                stroke_color,
                                fill_opacity,
                            },
                        )
                        .unwrap_or_else(|| {
                            build_shape_vello_path(
                                shape_type,
                                size,
                                line_from,
                                line_to,
                                arc_angles,
                                color,
                                stroke_width,
                                stroke_color,
                                fill_opacity,
                            )
                        });
                        vello_paths.push(vello_path);
                    }

                    if duration_ms > 0.0 {
                        let start_vector_paths = track.evaluate_vector_paths(t_start_ms);
                        let start_position = track.position.evaluate(t_start_ms);
                        let start_size = track.size.evaluate(t_start_ms);
                        let start_line_from = track.line_from.evaluate(t_start_ms);
                        let start_line_to = track.line_to.evaluate(t_start_ms);
                        let start_arc_angles = track.arc_angles.evaluate(t_start_ms);
                        let start_color = track.color.evaluate(t_start_ms);
                        let start_shape_type = track.shape_type.evaluate(t_start_ms);
                        let start_opacity = track.opacity.evaluate(t_start_ms);
                        let start_stroke_width = track.stroke_width.evaluate(t_start_ms);
                        let start_stroke_color = track.stroke_color.evaluate(t_start_ms);
                        let start_stroke_progress = track.stroke_progress.evaluate(t_start_ms);
                        let start_fill_opacity = track.fill_opacity.evaluate(t_start_ms);

                        track.vector_paths.add_keyframe(
                            t_start_ms,
                            start_vector_paths,
                            Easing::Linear,
                        );
                        track
                            .position
                            .add_keyframe(t_start_ms, start_position, Easing::Linear);
                        track
                            .size
                            .add_keyframe(t_start_ms, start_size, Easing::Linear);
                        track
                            .line_from
                            .add_keyframe(t_start_ms, start_line_from, Easing::Linear);
                        track
                            .line_to
                            .add_keyframe(t_start_ms, start_line_to, Easing::Linear);
                        track
                            .arc_angles
                            .add_keyframe(t_start_ms, start_arc_angles, Easing::Linear);
                        track
                            .color
                            .add_keyframe(t_start_ms, start_color, Easing::Linear);
                        track
                            .shape_type
                            .add_keyframe(t_start_ms, start_shape_type, Easing::Linear);
                        track
                            .opacity
                            .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
                        track.stroke_width.add_keyframe(
                            t_start_ms,
                            start_stroke_width,
                            Easing::Linear,
                        );
                        track.stroke_color.add_keyframe(
                            t_start_ms,
                            start_stroke_color,
                            Easing::Linear,
                        );
                        track.stroke_progress.add_keyframe(
                            t_start_ms,
                            start_stroke_progress,
                            Easing::Linear,
                        );
                        track.fill_opacity.add_keyframe(
                            t_start_ms,
                            start_fill_opacity,
                            Easing::Linear,
                        );
                    } else if delay_ms > 0.0 {
                        preserve_instant_delayed_value(&mut track.vector_paths, t_start_ms);
                        preserve_instant_delayed_value(&mut track.position, t_start_ms);
                        preserve_instant_delayed_value(&mut track.size, t_start_ms);
                        preserve_instant_delayed_value(&mut track.line_from, t_start_ms);
                        preserve_instant_delayed_value(&mut track.line_to, t_start_ms);
                        preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                        preserve_instant_delayed_value(&mut track.color, t_start_ms);
                        preserve_instant_delayed_value(&mut track.shape_type, t_start_ms);
                        preserve_instant_delayed_value(&mut track.opacity, t_start_ms);
                        preserve_instant_delayed_value(&mut track.stroke_width, t_start_ms);
                        preserve_instant_delayed_value(&mut track.stroke_color, t_start_ms);
                        preserve_instant_delayed_value(&mut track.stroke_progress, t_start_ms);
                        preserve_instant_delayed_value(&mut track.fill_opacity, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }

                    track
                        .vector_paths
                        .add_keyframe(t_end_ms, vello_paths, easing);
                    track.position.add_keyframe(t_end_ms, position, easing);
                    track.size.add_keyframe(t_end_ms, size, easing);
                    track.line_from.add_keyframe(t_end_ms, line_from, easing);
                    track.line_to.add_keyframe(t_end_ms, line_to, easing);
                    track.arc_angles.add_keyframe(t_end_ms, arc_angles, easing);
                    track.color.add_keyframe(t_end_ms, color, easing);
                    track.shape_type.add_keyframe(t_end_ms, shape_type, easing);
                    track.opacity.add_keyframe(t_end_ms, opacity, easing);
                    track
                        .stroke_width
                        .add_keyframe(t_end_ms, stroke_width, easing);
                    track
                        .stroke_color
                        .add_keyframe(t_end_ms, stroke_color, easing);
                    track
                        .stroke_progress
                        .add_keyframe(t_end_ms, stroke_progress, easing);
                    track
                        .fill_opacity
                        .add_keyframe(t_end_ms, fill_opacity, easing);

                    if primitive.is_layout_container() {
                        self.apply_container_layout(
                            label,
                            ty,
                            t_start_ms as f64,
                            gap,
                            align.as_deref(),
                            cols,
                        );
                    }
                }
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                } => self.process_assignment_statement(
                    target,
                    property,
                    value,
                    modifiers,
                    time_ms,
                    diagnostics,
                ),
                Stmt::Always { body } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::LabeledAlways { label: _, body } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::ForLoop {
                    var,
                    iterable,
                    body,
                } => {
                    for value in for_iter_values(iterable, &self.env) {
                        self.env.set(var, value);
                        self.process_body(time_ms, body, parent_label, diagnostics);
                    }
                }
                Stmt::Sequence { body } => {
                    self.process_sequence(time_ms, body, parent_label, diagnostics);
                }
                Stmt::Stagger { modifiers, body } => {
                    self.process_stagger(time_ms, modifiers, body, parent_label, diagnostics);
                }
                Stmt::Action(action) => {
                    process_action(action, time_ms, self, diagnostics);
                }
                _ => {}
            }
        }
    }
}
