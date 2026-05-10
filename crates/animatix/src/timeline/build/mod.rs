use super::*;
use crate::ast::{InlineItem, Property};
use crate::timeline::actor_kind::find_actor_kind;
use crate::timeline::vello_path::VelloPath;

mod plot;
mod keyframe_utils;

use keyframe_utils::{insert_end_keyframes, insert_start_keyframes, preserve_delayed_values};
use plot::{build_graph_axis_paths, build_plot_curve_paths, PlotCurveParams};

// === Build Entry Points ===

impl Timeline {
    fn register_container_metadata_and_apply_layout(
        &mut self,
        label: &str,
        container_ty: &str,
        time_ms: u64,
        gap: f32,
        align: Option<&str>,
        cols: Option<usize>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let child_order = self
            .tracks
            .get(label)
            .map(|t| t.children.clone())
            .unwrap_or_default();
        let layout_children = self.build_layout_children(label, container_ty, &child_order, diagnostics);

        self.container_metadata.insert(
            label.to_string(),
            ContainerMetadata {
                layout_type: LayoutType::from_container_ty(container_ty),
                gap,
                align: align.unwrap_or("center").to_string(),
                cols,
                child_order,
                layout_children,
            },
        );

        self.apply_container_layout(label, time_ms as f64, diagnostics);
    }

    pub fn build(ast: &[Stmt]) -> Self {
        Self::build_with_diagnostics(ast, &std::collections::HashMap::new()).output
    }

    pub fn build_with_diagnostics(
        ast: &[Stmt],
        namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    ) -> BuildReport<Self> {
        let mut timeline = Self::new();
        load_standard_library(&mut timeline.env);
        timeline.apply_colorscheme(BuiltInColorscheme::DefaultDark.resolved());
        let mut current_build_time_ms = 0.0;
        let mut diagnostics = Vec::new();

        timeline.load_colorscheme_declarations(ast, &mut diagnostics);

        // Seed environment with namespace exports
        for (alias, namespace) in namespaces {
            for (name, expr) in &namespace.exports {
                let key = format!("{}.{}", alias, name);
                // Evaluate the export expression in the current env
                match evaluate_expr(expr, &timeline.env) {
                    Ok(value) => {
                        timeline.env.set(&key, value);
                    }
                    Err(e) => {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::ModuleExportEvalError,
                                DiagnosticPhase::Build,
                                format!(
                                    "Failed to evaluate export '{}.{}': {}; using default.",
                                    alias, name, e
                                ),
                            )
                            .with_subject(&key),
                        );
                    }
                }
            }
        }

        for stmt in ast {
            if let Stmt::Config { settings, .. } = stmt {
                timeline.apply_config_settings(settings, &mut diagnostics);
            }
        }

        for stmt in ast {
            match stmt {
                Stmt::Config { .. } => {}
                Stmt::Keyframe { time, body, .. } => {
                    current_build_time_ms = time_to_ms(time);
                    timeline.process_body(current_build_time_ms, body, None, &mut diagnostics);
                }
                Stmt::RelativeKeyframe { offset, body, .. } => {
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

        // Compile always-body statements into IR for faster frame-time evaluation
        match crate::timeline::modifier_runtime::ir::lower_modifier_body(&timeline.modifiers) {
            Ok(program) => {
                timeline.modifier_programs.push(program);
            }
            Err(e) => {
                // Fall back to AST interpretation for this batch
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::ModifierCompilationError,
                    DiagnosticPhase::Build,
                    format!("Failed to compile always blocks to IR: {}; using AST fallback.", e),
                ));
            }
        }

        BuildReport::new(timeline, diagnostics)
    }

    // === Colorscheme Seeding ===

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

    // === Colorscheme Declaration Parsing ===

    fn load_colorscheme_declarations(&mut self, ast: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
        let mut schemes: std::collections::HashMap<String, ResolvedColorscheme> =
            std::collections::HashMap::new();
        let mut inheritance_edges: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for stmt in ast {
            if let Stmt::LetDecl { name, value, .. } = stmt {
                if let Expr::Construct(type_name, properties) = value {
                    if type_name == "Colorscheme" {
                        // Extract extends from properties
                        let mut extends = None;
                        let mut scheme_props = Vec::new();
                        for prop in properties {
                            if prop.name == "extends" {
                                if let Expr::Str(base) = &prop.value {
                                    extends = Some(base.clone());
                                }
                            } else {
                                scheme_props.push(prop.clone());
                            }
                        }
                        if let Some(scheme) = ResolvedColorscheme::from_properties(
                            name.clone(),
                            &scheme_props,
                            diagnostics,
                        ) {
                            if let Some(base_name) = extends {
                                inheritance_edges.insert(name.clone(), base_name);
                            }
                            schemes.insert(name.clone(), scheme);
                        }
                    }
                }
            }
        }

        let mut resolved: std::collections::HashMap<String, ResolvedColorscheme> =
            std::collections::HashMap::new();

        for name in schemes.keys() {
            if let Some(scheme) = self.resolve_colorscheme_with_inheritance(
                name,
                &schemes,
                &inheritance_edges,
                &mut resolved,
                &mut std::collections::HashSet::new(),
                diagnostics,
            ) {
                resolved.insert(name.clone(), scheme);
            }
        }

        self.external_colorschemes = resolved;
    }

    // === Colorscheme Inheritance Resolution ===

    fn resolve_colorscheme_with_inheritance(
        &self,
        name: &str,
        schemes: &std::collections::HashMap<String, ResolvedColorscheme>,
        edges: &std::collections::HashMap<String, String>,
        resolved: &mut std::collections::HashMap<String, ResolvedColorscheme>,
        visiting: &mut std::collections::HashSet<String>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<ResolvedColorscheme> {
        if let Some(scheme) = resolved.get(name) {
            return Some(scheme.clone());
        }

        if !visiting.insert(name.to_string()) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::ColorschemeInheritanceCycle,
                    DiagnosticPhase::Build,
                    format!(
                        "Colorscheme inheritance cycle detected involving '{}'.",
                        name
                    ),
                )
                .with_subject(name),
            );
            return None;
        }

        let mut scheme = schemes.get(name)?.clone();

        if let Some(base_name) = edges.get(name) {
            if let Some(base_builtin) = BuiltInColorscheme::from_name(base_name) {
                scheme.merge_with_base(&base_builtin.resolved());
            } else if let Some(base_resolved) = self.resolve_colorscheme_with_inheritance(
                base_name,
                schemes,
                edges,
                resolved,
                visiting,
                diagnostics,
            ) {
                scheme.merge_with_base(&base_resolved);
            } else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownColorscheme,
                        DiagnosticPhase::Build,
                        format!(
                            "Colorscheme '{}' extends unknown base '{}'; using as-is.",
                            name, base_name
                        ),
                    )
                    .with_subject(name),
                );
            }
        }

        visiting.remove(name);
        Some(scheme)
    }

    // === Config Processing ===

    fn apply_config_settings(
        &mut self,
        settings: &[crate::ast::Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for setting in settings {
            if setting.name == "dynamic_layout" {
                self.dynamic_layout = match &setting.value {
                    Expr::Bool(b) => *b,
                    Expr::Str(s) => s.parse().unwrap_or(false),
                    _ => false,
                };
                continue;
            }

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

            if let Some(built_in) = BuiltInColorscheme::from_name(&raw_name) {
                self.apply_colorscheme(built_in.resolved());
                continue;
            }

            if let Some(external) = self.external_colorschemes.get(&raw_name).cloned() {
                self.apply_colorscheme(external);
                continue;
            }

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

    // === Scene Graph Node Creation ===

    pub(super) fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        if let Some(parent) = parent_label {
            self.root_nodes.retain(|root| root != &label);

            // Add child to parent's children list
            let parent_track = self.tracks
                .entry(parent.to_string())
                .or_insert_with(|| AnimationTrack::new(parent.to_string()));
            if !parent_track.children.contains(&label) {
                parent_track.children.push(label.clone());
            }
        } else {
            let already_nested = self
                .tracks
                .values()
                .any(|track| track.children.contains(&label));

            // No parent → root node, unless the actor already belongs to a container
            if !already_nested && !self.root_nodes.contains(&label) {
                self.root_nodes.push(label.clone());
            }
        }
    }

    // === Children Processing ===

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
                        span: None,
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
                        span: None,
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                }
                // SlotMarker and SlotFill are resolved during component expansion.
                // At timeline build time they should never appear in the AST.
                crate::ast::InlineItem::SlotMarker { .. }
                | crate::ast::InlineItem::SlotFill { .. } => {
                    // Unreachable after correct component expansion.
                    // Emitting a diagnostic here is noisy for a correctness-invariant;
                    // if they appear, the timeline simply ignores them.
                }
            }
        }
    }

    // === Shape Actor Processing ===

    #[allow(clippy::too_many_arguments)]
    fn process_shape_actor(
        _label: &str,
        ty: &str,
        _time_ms: f64,
        _parent_label: Option<&str>,
        _diagnostics: &mut Vec<Diagnostic>,
        _existing_track: &AnimationTrack,
        _vector_shape_state: &mut VectorShapeState,
        size: [f32; 2],
        line_from: [f32; 2],
        line_to: [f32; 2],
        arc_angles: [f32; 2],
        color: [f32; 4],
        stroke_width: f32,
        stroke_color: [f32; 4],
        _stroke_progress: f32,
        fill_opacity: f32,
        shape_type: ShapeType,
    ) -> Vec<VelloPath> {
        let primitive = PrimitiveDescriptor::for_actor_type(ty);
        if primitive.is_plot() {
            return vec![];
        }

        let vello_path = build_vector_shape_vello_path(
            shape_type,
            _vector_shape_state,
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
        vec![vello_path]
    }

    // === ActorKind Dispatch Methods ===

    /// Dispatch method for plot actor kinds (called from ActorKind trait impl)
    pub(super) fn process_plot_actor_dispatch(
        &mut self,
        label: &str,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let existing_track = self
            .tracks
            .get(label)
            .cloned()
            .unwrap_or_else(|| AnimationTrack::new(label.to_string()));

        if let Some((
            initial_size,
            line_from,
            line_to,
            arc_angles,
            color,
            stroke_width,
            stroke_color,
            stroke_progress,
            fill_opacity,
            shape_type,
            vello_paths,
        )) = self.process_plot_actor(
            label,
            ty,
            props,
            time_ms,
            parent_label,
            children,
            diagnostics,
            &existing_track,
        ) {
            // Use returned values for keyframe insertion
            let position = existing_track.position.last([0.0, 0.0]);
            let size = initial_size;
            let opacity = 1.0;

            let ParsedTimingModifiers {
                duration_ms,
                delay_ms,
                easing,
                morph_options: _,
            } = parse_timing_modifiers(
                modifiers,
                ModifierHost::ActorDeclaration,
                Some(label),
                diagnostics,
            );
            let t_start_ms = (time_ms + delay_ms) as u64;
            let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

            let track = self
                .tracks
                .entry(label.to_string())
                .or_insert_with(|| AnimationTrack::new(label.to_string()));
            track.kind = super::ActorKindId::from_type_name(ty);

            if track.first_seen_ms == u64::MAX {
                track.first_seen_ms = t_start_ms;
            }

            // === Keyframe Insertion ===
            if duration_ms > 0.0 {
                insert_start_keyframes(track, t_start_ms);
            } else if delay_ms > 0.0 {
                preserve_delayed_values(track, t_start_ms);
            }

            insert_end_keyframes(
                track,
                t_end_ms,
                position,
                size,
                line_from,
                line_to,
                arc_angles,
                color,
                shape_type,
                opacity,
                stroke_width,
                stroke_color,
                stroke_progress,
                fill_opacity,
                vello_paths,
                easing,
            );

            // === Container Layout ===
            let primitive = PrimitiveDescriptor::for_actor_type(ty);
            if primitive.is_layout_container() {
                self.register_container_metadata_and_apply_layout(
                    label,
                    ty,
                    t_start_ms,
                    0.0,
                    Some("center"),
                    None,
                    diagnostics,
                );
            }
        }
    }

    // === Main AST Statement Processor ===

    pub(super) fn process_body(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for stmt in body {
            match stmt {
                Stmt::Text { .. } | Stmt::Math { .. } | Stmt::Code { .. } => {
                    self.process_text_like_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::Svg { .. } | Stmt::Image { .. } => {
                    self.process_media_statement(stmt, time_ms, parent_label, diagnostics)
                }
                Stmt::ActorDecl {
                    is_pub: _,
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                    ..
                } => {
                    self.add_node(label.clone(), parent_label);

                    // Try trait-based dispatch first
                    if let Some(kind) = find_actor_kind(ty) {
                        kind.build(self, label, ty, props, modifiers, children, time_ms, parent_label, diagnostics);
                        continue;
                    }

                    // Fallback: process as unknown actor type
                    // (for built-in types, find_actor_kind should always return Some)
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

                    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
                    let default_arc = [0.0, std::f32::consts::PI];
                    let mut position = existing_track.position.last([0.0, 0.0]);
                    let mut size = existing_track.size.last(default_size);
                    let mut line_from = existing_track.line_from.last([-50.0, 0.0]);
                    let mut line_to = existing_track.line_to.last([50.0, 0.0]);
                    let mut arc_angles = existing_track.arc_angles.last(default_arc);
                    let mut color = existing_track.color.last([1.0, 1.0, 1.0, 1.0]);
                    let vector_shape = vector_shape_primitive_for_actor_type(ty);
                    let shape_type = shape_type_for_actor(ty);
                    let opacity = existing_track.opacity.last(1.0);
                    let mut stroke_width = existing_track.stroke_width.last(2.0);
                    let mut stroke_color = existing_track.stroke_color.last([1.0, 1.0, 1.0, 1.0]);
                    let mut stroke_progress = existing_track.stroke_progress.last(1.0);
                    let mut fill_opacity = existing_track.fill_opacity.last(1.0);
                    let mut gap = 0.0f32;
                    let mut align: Option<String> = None;
                    let mut cols: Option<usize> = None;
                    let mut vector_shape_state =
                        VectorShapeState::new(size, line_from, line_to, arc_angles);
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
                        existing_track.vector_paths.as_ref().map(|t| !t.keyframes.is_empty()).unwrap_or(false) && duration_ms > 0.0;

                    if has_non_default_morph_options(morph_options) && !supports_morph_options {
                        push_modifier_diagnostic(
                            diagnostics,
                            DiagnosticCode::InvalidModifierValue,
                            "Morph-specific modifiers on actor declarations require a path-morphing re-declaration with non-zero duration; ignoring them for now.".to_string(),
                            Some(label),
                        );
                    }

                    // Apply scheme-appropriate default colors when no explicit color/stroke property is provided
                    let has_explicit_color = props.iter().any(|p| p.name == "color");
                    let has_explicit_stroke = props
                        .iter()
                        .any(|p| p.name == "stroke" || p.name == "stroke_color");
                    let default_white = [1.0, 1.0, 1.0, 1.0];

                    if !has_explicit_color && color == default_white {
                        if let Some(scheme_color) = self.get_default_color(ty, "color") {
                            color = scheme_color;
                            if primitive.is_plot_curve() {
                                stroke_color = scheme_color;
                            }
                        }
                    }
                    if !has_explicit_stroke && stroke_color == default_white {
                        if let Some(scheme_stroke) = self.get_default_color(ty, "stroke") {
                            stroke_color = scheme_stroke;
                        }
                    }

                    for prop in props {
                        let prop_subject = format!("{}.{}", label, prop.name);
                        match prop.name.as_str() {
                            "at" | "anchor" | "offset" => {}
                            // ── Special cases with complex logic ──
                            "radius" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(0.0));
                                let r = v.as_num() as f32;
                                size = [r, r];
                                vector_shape_state.size = size;
                                vector_shape_state.regular_polygon_radius = r;
                            }
                            "size" => {
                                let size_val = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(0.0));
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
                                        if primitive.is_plot_curve() { stroke_color = actor_color; }
                                    } else {
                                        diagnostics.push(Diagnostic::warning(
                                            DiagnosticCode::UnknownColorReference,
                                            DiagnosticPhase::Build,
                                            format!("Color value 'auto' on '{}.color' requests automatic colorscheme assignment, but the selected colorscheme has no auto-assignment colors; keeping the existing/default color instead.", label),
                                        ).with_subject(&prop_subject));
                                    }
                                } else if let Some(resolved_color) =
                                    parse_color_in_env_with_lookup_diagnostic(
                                        label, "color", &prop.value, &eval_env, diagnostics, &prop_subject,
                                    ) {
                                    color = resolved_color;
                                    if primitive.is_plot_curve() { stroke_color = resolved_color; }
                                }
                            }
                            "gap" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(0.0));
                                gap = v.as_num() as f32;
                            }
                            "align" => {
                                if let Expr::Str(s) = &prop.value { align = Some(s.clone()); }
                                else if let Expr::Ident(s) = &prop.value { align = Some(s.clone()); }
                            }
                            "cols" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(1.0));
                                cols = Some(v.as_num().max(1.0) as usize);
                            }
                            // ── Registry-driven simple properties ──
                            // These must come BEFORE the vector_shape catch-all to avoid
                            // being swallowed by `_ if vector_shape.is_some()`.
                            "stroke_width" | "width" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(0.0));
                                stroke_width = v.as_num() as f32;
                            }
                            "stroke_color" | "stroke" => {
                                if let Some(resolved_color) = parse_color_in_env_with_lookup_diagnostic(
                                    label, "stroke_color", &prop.value, &eval_env, diagnostics, &prop_subject,
                                ) { stroke_color = resolved_color; }
                            }
                            "stroke_progress" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(0.0));
                                stroke_progress = v.as_num() as f32;
                            }
                            "fill_opacity" => {
                                let v = evaluate_expr_with_lookup_diagnostic(
                                    &prop.value, &eval_env, diagnostics, &prop_subject,
                                ).unwrap_or(Value::Num(0.0));
                                fill_opacity = v.as_num() as f32;
                            }
                            // ── Vector shape properties (catch-all for unhandled props) ──
                            _ if vector_shape.is_some() => {
                                if apply_vector_shape_property(
                                    ty, &prop.name, &prop.value, &eval_env, diagnostics, &prop_subject,
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

                    // For Graph types and layout containers, make them invisible (container only)
                    if primitive.is_graph_host() || primitive.is_layout_container() {
                        fill_opacity = 0.0;
                        stroke_width = 0.0;
                    }

                    let track = self
                        .tracks
                        .entry(label.clone())
                        .or_insert_with(|| AnimationTrack::new(label.clone()));
                    track.kind = super::ActorKindId::from_type_name(ty);

                    // Record first declaration time so scene evaluation can hide
                    // actors before they are declared
                    if track.first_seen_ms == u64::MAX {
                        track.first_seen_ms = t_start_ms;
                    }

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
                        vello_paths = build_graph_axis_paths(size, x_domain, y_domain);
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

                        let curve_params = PlotCurveParams {
                            ty,
                            func: &func,
                            p_x_domain,
                            p_y_domain,
                            p_size,
                            t_domain,
                            tolerance,
                            max_depth,
                            resolution,
                            stroke_width,
                            stroke_color,
                            eval_env: &eval_env,
                        };
                        vello_paths = build_plot_curve_paths(&curve_params);
                    } else if !primitive.is_plot() {
                        vello_paths = Self::process_shape_actor(
                            label,
                            ty,
                            time_ms,
                            parent_label,
                            diagnostics,
                            &existing_track,
                            &mut vector_shape_state,
                            size,
                            line_from,
                            line_to,
                            arc_angles,
                            color,
                            stroke_width,
                            stroke_color,
                            stroke_progress,
                            fill_opacity,
                            shape_type,
                        );
                    }

                    if duration_ms > 0.0 {
                        insert_start_keyframes(track, t_start_ms);
                    } else if delay_ms > 0.0 {
                        preserve_delayed_values(track, t_start_ms);
                    }
                    if supports_morph_options {
                        track
                            .morph_options
                            .ensure(MorphOptions::default())
                            .add_keyframe(t_end_ms, morph_options, Easing::Linear);
                    }

                    insert_end_keyframes(
                        track,
                        t_end_ms,
                        position,
                        size,
                        line_from,
                        line_to,
                        arc_angles,
                        color,
                        shape_type,
                        opacity,
                        stroke_width,
                        stroke_color,
                        stroke_progress,
                        fill_opacity,
                        vello_paths,
                        easing,
                    );

                    // === Container Layout ===

                    if primitive.is_layout_container() {
                        self.register_container_metadata_and_apply_layout(
                            label,
                            ty,
                            t_start_ms,
                            gap,
                            align.as_deref(),
                            cols,
                            diagnostics,
                        );
                    }
                }
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                    value_span: _,
                    ..
                } => self.process_assignment_statement(
                    target,
                    property,
                    value,
                    modifiers,
                    time_ms,
                    diagnostics,
                ),
                Stmt::Always { body, .. } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::LabeledAlways { label: _, body, .. } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::ForLoop {
                    var,
                    iterable,
                    body,
                    ..
                } => {
                    for value in for_iter_values(iterable, &self.env) {
                        self.env.set(var, value);
                        self.process_body(time_ms, body, parent_label, diagnostics);
                    }
                }
                Stmt::Sequence { body, .. } => {
                    self.process_sequence(time_ms, body, parent_label, diagnostics);
                }
                Stmt::Stagger { modifiers, body, .. } => {
                    self.process_stagger(time_ms, modifiers, body, parent_label, diagnostics);
                }
                Stmt::Action(action, span) => {
                    process_action(action, time_ms, self, diagnostics, *span);
                }
                _ => {}
            }
        }
    }
}
