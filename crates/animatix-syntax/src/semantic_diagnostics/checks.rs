use super::*;

pub(super) fn check_stmt(
    stmt: &Stmt,
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Action(action, span) => {
            let (line, col, end_line, end_col) = span_positions(span);
            let byte_range = action.byte_span.map(|s| s.start..s.end);

            if !symbols.actions.contains(&action.verb) {
                let (vstart, vend) =
                    find_ident_range(tokens, source, &action.verb, byte_range.clone())
                        .unwrap_or_else(|| ((line - 1, col - 1), (end_line - 1, end_col - 1)));
                diagnostics.push(range_diagnostic(
                    DiagnosticSeverity::Warning,
                    DiagnosticCode::UnknownAction,
                    format!("Unknown action: {}", action.verb),
                    vstart,
                    vend,
                ));
            }

            for target in &action.targets {
                // Dotted targets (`eq.lhs`, `row.b[j]` projected to `row.b`) are
                // resolved by walking the scene hierarchy at build time, so only
                // the root label needs to be defined here — mirroring the
                // assignment-target check.
                let check_label = target.split('.').next().unwrap_or(target);
                // `persist camera` / `remove camera` address the reserved scene
                // camera, which is never a declared actor. Gated on the verb
                // because no other action has an engine path for it.
                let is_persisted_camera = check_label == animatix_core::property::CAMERA_TARGET
                    && matches!(action.verb.as_str(), "persist" | "remove");
                let is_defined = is_persisted_camera
                    || symbols.labels.contains_key(check_label)
                    || is_array_member_label(check_label)
                        .is_some_and(|base| symbols.array_labels.contains(base))
                    || is_component_array_member(symbols, check_label);
                if !is_defined {
                    let source_text = target_source_text(target);
                    let (tstart, tend) =
                        find_ident_range(tokens, source, source_text, byte_range.clone())
                            .unwrap_or_else(|| ((line - 1, col - 1), (end_line - 1, end_col - 1)));
                    diagnostics.push(range_diagnostic(
                        DiagnosticSeverity::Warning,
                        DiagnosticCode::UndefinedLabel,
                        format!("Undefined label: {}", check_label),
                        tstart,
                        tend,
                    ));
                }
            }
        },

        Stmt::Assignment {
            target,
            property,
            value,
            span,
            ..
        } => {
            let (line, col, _end_line, end_col) = span_positions(span);

            if let Some(seg) = target.first() {
                let label = seg.label_str();
                // `camera` is the reserved scene-camera target, never a declared
                // actor; the engine routes its assignments without a track.
                let is_defined = label == animatix_core::property::CAMERA_TARGET
                    || symbols.labels.contains_key(label)
                    || is_array_member_label(label)
                        .is_some_and(|base| symbols.array_labels.contains(base))
                    || is_component_array_member(symbols, label);
                if !is_defined {
                    diagnostics.push(span_diagnostic(
                        DiagnosticSeverity::Warning,
                        DiagnosticCode::UndefinedLabel,
                        format!("Undefined label: {}", label),
                        line,
                        col,
                        end_col,
                    ));
                }
            }

            // `scope.stage.param = value` addresses an effect stage parameter,
            // which is not on the scope's property table. Validate it against
            // the stage's declared effect — built-in or manifest-declared —
            // resolved through the stage label's collected type. A bare
            // `stage.param = value` resolves the same way when the stage is
            // the direct target.
            let stage_label = target.last().map(|seg| seg.label_str());
            let effect_ty = stage_label
                .and_then(|label| symbols.inline_child_type(label))
                .filter(|ty| symbols.is_effect_type(ty))
                .map(str::to_string);
            if let Some(effect_ty) = effect_ty {
                if let Some(known_props) = symbols.properties.get(&effect_ty) {
                    if !known_props.contains(property) {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Info,
                            DiagnosticCode::UnknownProperty,
                            format!("Effect '{}' has no parameter '{}'", effect_ty, property),
                            line,
                            col,
                            end_col,
                        ));
                    }

                    let key = (effect_ty.clone(), property.to_string());
                    if let Some(expected_type) = symbols.property_types.get(&key) {
                        let actual_type = symbols.infer_expr_type(value);
                        if !crate::typing::is_subtype(&actual_type, expected_type) {
                            diagnostics.push(span_diagnostic(
                                DiagnosticSeverity::Warning,
                                DiagnosticCode::TypeMismatch,
                                format!(
                                    "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                    effect_ty, property, expected_type, actual_type
                                ),
                                line,
                                col,
                                end_col,
                            ));
                        }
                    }
                }
            } else if let Some(param) = if target.len() >= 2 {
                crate::schema::effect_specs().iter().find_map(|spec| {
                    spec.params.iter().find(|param| param.name.as_ref() == property)
                })
            } else {
                None
            } {
                if let Some(expected_type) = crate::symbol_table::effect_param_type(param.kind) {
                    let actual_type = symbols.infer_expr_type(value);
                    if !crate::typing::is_subtype(&actual_type, &expected_type) {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Warning,
                            DiagnosticCode::TypeMismatch,
                            format!(
                                "Type mismatch for effect parameter '{}': expected {:?}, found {:?}",
                                property, expected_type, actual_type
                            ),
                            line,
                            col,
                            end_col,
                        ));
                    }
                }
            } else if let Some(stage_label) = stage_param_segment(target, symbols, property) {
                // A `scope.stage.param` target whose stage type is not a known
                // effect (typically a plugin effect whose library is not
                // loaded) is still *stage-parameter* position, not a host
                // property assignment. Checking it against the host actor's
                // property table produced false `type-mismatch` warnings — e.g.
                // `panel.pix.size = 6` compared against `Filter.size` (Vec2)
                // because `Pixelate` was unknown. Stay silent instead: the
                // unknown effect type is already reported elsewhere.
                let _ = stage_label;
            } else {
                let resolved = target.first().and_then(|seg| symbols.labels.get(seg.label_str()));
                let label = target.first().map(|seg| seg.label_str()).unwrap_or("");
                if let Some(info) = resolved {
                    if let Some(ty) = &info.ty {
                        if let Some(known_props) = symbols.properties.get(ty) {
                            if !known_props.contains(property)
                                && !info.plot_params.iter().any(|p| p == property)
                            {
                                diagnostics.push(span_diagnostic(
                                    DiagnosticSeverity::Info,
                                    DiagnosticCode::UnknownProperty,
                                    format!(
                                        "Property '{}' not commonly used on {} (may still be valid)",
                                        property, ty
                                    ),
                                    line,
                                    col,
                                    end_col,
                                ));
                            }

                            let key = (ty.clone(), property.clone());
                            if let Some(expected_type) = symbols.property_types.get(&key) {
                                let actual_type = symbols.infer_expr_type(value);
                                if !crate::typing::is_subtype(&actual_type, expected_type) {
                                    diagnostics.push(span_diagnostic(
                                        DiagnosticSeverity::Warning,
                                        DiagnosticCode::TypeMismatch,
                                        format!(
                                            "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                            label, property, expected_type, actual_type
                                        ),
                                        line,
                                        col,
                                        end_col,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        },

        Stmt::ActorDecl {
            label,
            ty,
            props,
            span,
            ..
        } => {
            let (line, col, _end_line, end_col) = span_positions(span);
            // Actor declaration spans are not populated by the parser, so a
            // diagnostic anchored on the declaration would fall back to the
            // file start. Locate this declaration's own `label: Type` token
            // pair instead, so each of several same-typed declarations points
            // at its own line rather than the first one in the file.
            let type_range = find_decl_type_range(tokens, source, label, ty);

            // Component instances are valid actor types too: accept local and
            // imported (`pub component`) components as well as namespaced ones
            // (`import as` + `alias.Component`), not just builtin/primitive types.
            let ty_known = symbols.types.contains(ty)
                || symbols.components.contains_key(ty)
                || symbols.resolve_namespaced_component(ty).is_some();
            if !ty_known {
                // A genuinely unknown type cannot render, and the build layer
                // rejects it as an error (`unknown-actor-type`). Report the
                // same severity here so the editor and the CLI agree; only a
                // registered primitive whose extension plugin is missing stays
                // a warning, and that case is not visible at this layer.
                let (ty_line, ty_col, ty_end_col) = match type_range {
                    Some((start, end)) => (start.0 + 1, start.1 + 1, end.1 + 1),
                    None => (line, col, end_col),
                };
                diagnostics.push(span_diagnostic(
                    DiagnosticSeverity::Error,
                    DiagnosticCode::UnknownType,
                    format!("Unknown type: {}", ty),
                    ty_line,
                    ty_col,
                    ty_end_col,
                ));
            }

            if let Some(known_props) = symbols.properties.get(ty) {
                for prop in props {
                    // Point at the property's own value span so the squiggle
                    // lands on the offending property rather than the actor
                    // declaration's first character.
                    let (prop_line, prop_col, prop_end_line, prop_end_col) =
                        property_positions(prop, source, line, col, end_col);
                    if !known_props.contains(&prop.name)
                        && !is_plot_runtime_param(ty, &prop.name, props)
                    {
                        diagnostics.push(span_diagnostic(
                            DiagnosticSeverity::Info,
                            DiagnosticCode::UnknownProperty,
                            format!(
                                "Property '{}' not commonly used on {} (may still be valid)",
                                prop.name, ty
                            ),
                            prop_line,
                            prop_col,
                            prop_end_col,
                        ));
                        if let Some(last) = diagnostics.last_mut() {
                            last.location.end_line = Some(prop_end_line);
                        }
                    }

                    let key = (ty.clone(), prop.name.clone());
                    if let Some(expected_type) = symbols.property_types.get(&key) {
                        let actual_type = symbols.infer_expr_type(&prop.value);
                        if !crate::typing::is_subtype(&actual_type, expected_type) {
                            diagnostics.push(span_diagnostic(
                                DiagnosticSeverity::Warning,
                                DiagnosticCode::TypeMismatch,
                                format!(
                                    "Type mismatch for '{}.{}': expected {:?}, found {:?}",
                                    ty, prop.name, expected_type, actual_type
                                ),
                                prop_line,
                                prop_col,
                                prop_end_col,
                            ));
                            if let Some(last) = diagnostics.last_mut() {
                                last.location.end_line = Some(prop_end_line);
                            }
                        }
                    }
                }
            }
        },

        _ => {},
    }
}
