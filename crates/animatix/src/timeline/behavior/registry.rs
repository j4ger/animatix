//! BehaviorRegistry storing built-in and extension behavior definitions.

use std::sync::Arc;

use super::builtin::*;
use super::def::{BehaviorDef, BehaviorFamily, BehaviorId, BehaviorSource, FamilyBehavior};

/// Return the canonical [`FamilyBehavior`] for a built-in primitive by type name.
pub fn family_behavior_for_builtin(name: &str) -> Option<FamilyBehavior> {
    match name {
        // Shapes
        "Rect" => Some(FamilyBehavior {
            evaluate: shape::evaluate_rect,
            build_extra: None,
        }),
        "Ellipse" => Some(FamilyBehavior {
            evaluate: shape::evaluate_ellipse,
            build_extra: None,
        }),
        "Line" => Some(FamilyBehavior {
            evaluate: shape::evaluate_line,
            build_extra: None,
        }),
        "Arrow" => Some(FamilyBehavior {
            evaluate: shape::evaluate_arrow,
            build_extra: None,
        }),
        "Polygon" => Some(FamilyBehavior {
            evaluate: shape::evaluate_polygon,
            build_extra: None,
        }),
        "Path" => Some(FamilyBehavior {
            evaluate: shape::evaluate_path,
            build_extra: None,
        }),

        // Text
        "Text" => Some(FamilyBehavior {
            evaluate: text::evaluate_text,
            build_extra: Some(text::build_extra_text),
        }),
        "Code" => Some(FamilyBehavior {
            evaluate: text::evaluate_code,
            build_extra: Some(text::build_extra_code),
        }),
        "Math" => Some(FamilyBehavior {
            evaluate: text::evaluate_math,
            build_extra: Some(text::build_extra_math),
        }),
        "Typst" => Some(FamilyBehavior {
            evaluate: text::evaluate_typst,
            build_extra: Some(text::build_extra_typst),
        }),
        "Counter" => Some(FamilyBehavior {
            evaluate: text::evaluate_counter,
            build_extra: Some(text::build_extra_counter),
        }),

        // Media
        "Image" => Some(FamilyBehavior {
            evaluate: media::evaluate_image,
            build_extra: Some(media::build_extra_image),
        }),
        "Svg" => Some(FamilyBehavior {
            evaluate: media::evaluate_svg,
            build_extra: Some(media::build_extra_svg),
        }),
        "Audio" => Some(FamilyBehavior {
            evaluate: media::evaluate_audio,
            build_extra: Some(media::build_extra_audio),
        }),

        // Plots
        "Graph" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_graph),
        }),
        "PlotCurve" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_plot_curve),
        }),
        "VectorField" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_vector_field),
        }),
        "Heatmap" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_heatmap),
        }),
        "ContourSet" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_contour_set),
        }),
        "NumberPlane" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_number_plane),
        }),
        "BarChart" => Some(FamilyBehavior {
            evaluate: plot::evaluate_plot,
            build_extra: Some(plot::build_extra_bar_chart),
        }),

        // Containers
        "Row" => Some(FamilyBehavior {
            evaluate: container::evaluate_container,
            build_extra: None,
        }),
        "Col" => Some(FamilyBehavior {
            evaluate: container::evaluate_container,
            build_extra: None,
        }),
        "Grid" => Some(FamilyBehavior {
            evaluate: container::evaluate_container,
            build_extra: None,
        }),
        "Stack" => Some(FamilyBehavior {
            evaluate: container::evaluate_container,
            build_extra: None,
        }),
        "Group" => Some(FamilyBehavior {
            evaluate: container::evaluate_container,
            build_extra: None,
        }),

        // Effects
        "Mask" => Some(FamilyBehavior {
            evaluate: effect::evaluate_effect,
            build_extra: None,
        }),
        "Filter" => Some(FamilyBehavior {
            evaluate: effect::evaluate_effect,
            build_extra: None,
        }),
        "Glass" => Some(FamilyBehavior {
            evaluate: effect::evaluate_effect,
            build_extra: None,
        }),

        // Equations
        "Equation" => Some(FamilyBehavior {
            evaluate: equation::evaluate_equation,
            build_extra: None,
        }),
        "Fragment" => Some(FamilyBehavior {
            evaluate: equation::evaluate_fragment,
            build_extra: Some(equation::build_extra_fragment),
        }),

        // Captions
        "Captions" => Some(FamilyBehavior {
            evaluate: captions::evaluate_captions,
            build_extra: None,
        }),
        "Cue" => Some(FamilyBehavior {
            evaluate: captions::evaluate_cue,
            build_extra: Some(captions::build_extra_cue),
        }),

        // Annotations
        "Callout" => Some(FamilyBehavior {
            evaluate: annotation::evaluate_callout,
            build_extra: None,
        }),
        "Legend" => Some(FamilyBehavior {
            evaluate: annotation::evaluate_legend,
            build_extra: Some(annotation::build_extra_legend),
        }),
        "Connector" => Some(FamilyBehavior {
            evaluate: annotation::evaluate_connector,
            build_extra: None,
        }),

        _ => None,
    }
}

/// Static lookup table mapping built-in primitive type names to their canonical [`BehaviorId`].
///
/// Order matches [`crate::primitives::BUILT_INS`] (indices `0..36`).
pub fn behavior_id_for_builtin(name: &str) -> Option<BehaviorId> {
    match name {
        "Rect" => Some(BehaviorId(0)),
        "Ellipse" => Some(BehaviorId(1)),
        "Line" => Some(BehaviorId(2)),
        "Arrow" => Some(BehaviorId(3)),
        "Polygon" => Some(BehaviorId(4)),
        "Path" => Some(BehaviorId(5)),
        "Text" => Some(BehaviorId(6)),
        "Code" => Some(BehaviorId(7)),
        "Math" => Some(BehaviorId(8)),
        "Typst" => Some(BehaviorId(9)),
        "Image" => Some(BehaviorId(10)),
        "Svg" => Some(BehaviorId(11)),
        "Audio" => Some(BehaviorId(12)),
        "Graph" => Some(BehaviorId(13)),
        "PlotCurve" => Some(BehaviorId(14)),
        "VectorField" => Some(BehaviorId(15)),
        "Heatmap" => Some(BehaviorId(16)),
        "ContourSet" => Some(BehaviorId(17)),
        "NumberPlane" => Some(BehaviorId(18)),
        "BarChart" => Some(BehaviorId(19)),
        "Row" => Some(BehaviorId(20)),
        "Col" => Some(BehaviorId(21)),
        "Grid" => Some(BehaviorId(22)),
        "Stack" => Some(BehaviorId(23)),
        "Group" => Some(BehaviorId(24)),
        "Mask" => Some(BehaviorId(25)),
        "Filter" => Some(BehaviorId(26)),
        "Glass" => Some(BehaviorId(27)),
        "Equation" => Some(BehaviorId(28)),
        "Fragment" => Some(BehaviorId(29)),
        "Captions" => Some(BehaviorId(30)),
        "Cue" => Some(BehaviorId(31)),
        "Callout" => Some(BehaviorId(32)),
        "Legend" => Some(BehaviorId(33)),
        "Connector" => Some(BehaviorId(34)),
        "Counter" => Some(BehaviorId(35)),
        _ => None,
    }
}

/// Registry holding behavior definitions indexed by [`BehaviorId`].
///
/// Seeded with all 36 built-in primitives at startup (indices `0..36`).
/// Supports registering extension primitives at runtime.
#[derive(Clone)]
pub struct BehaviorRegistry {
    behaviors: Vec<BehaviorDef>,
}

impl Default for BehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BehaviorRegistry {
    /// Create a new registry pre-seeded with all built-in primitives.
    pub fn new() -> Self {
        let mut behaviors = Vec::with_capacity(crate::primitives::BUILT_INS.len());
        for (i, builtin) in crate::primitives::BUILT_INS.iter().enumerate() {
            let caps = animatix_std::caps_from_info(builtin.info);
            let family = BehaviorFamily::from_caps(&caps);
            let child_processing = caps.child_processing;
            let name: Box<str> = builtin.behavior.type_name().into();
            let id = BehaviorId(i as u16);
            let family_behavior = family_behavior_for_builtin(&name)
                .unwrap_or_else(|| panic!("missing FamilyBehavior for built-in '{name}'"));
            behaviors.push(BehaviorDef {
                id,
                name,
                caps,
                family,
                child_processing,
                source: BehaviorSource::Family(family_behavior),
            });
        }
        Self { behaviors }
    }

    /// Register a runtime extension primitive with its identity card.
    ///
    /// If an extension with the same type name was already registered, returns its existing ID.
    pub fn register_extension(
        &mut self,
        primitive: Arc<dyn crate::primitives::Primitive>,
        info: animatix_std::PrimitiveInfo,
    ) -> BehaviorId {
        let name = primitive.type_name();
        if let Some(existing) = self.find(name) {
            return existing.id;
        }
        let id = BehaviorId(self.behaviors.len() as u16);
        let caps = animatix_std::caps_from_info(&info);
        let family = BehaviorFamily::from_caps(&caps);
        let child_processing = caps.child_processing;
        self.behaviors.push(BehaviorDef {
            id,
            name: name.into(),
            caps,
            family,
            child_processing,
            source: BehaviorSource::Extension(primitive),
        });
        id
    }

    /// Look up a behavior definition by ID in O(1) time.
    #[inline(always)]
    pub fn get(&self, id: BehaviorId) -> Option<&BehaviorDef> {
        self.behaviors.get(id.index())
    }

    /// Look up a behavior definition by type name.
    pub fn find(&self, name: &str) -> Option<&BehaviorDef> {
        self.behaviors.iter().find(|b| &*b.name == name)
    }

    /// Fast lookup of a built-in behavior ID by type name without searching.
    #[inline(always)]
    pub fn behavior_id_for_builtin(name: &str) -> Option<BehaviorId> {
        behavior_id_for_builtin(name)
    }

    /// Number of registered behavior definitions.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.behaviors.len()
    }

    /// True when no behaviors are registered.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.behaviors.is_empty()
    }

    /// Iterate all behavior definitions in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &BehaviorDef> {
        self.behaviors.iter()
    }
}
