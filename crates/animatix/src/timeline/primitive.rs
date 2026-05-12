use crate::primitives::{find_primitive, Primitive};
use crate::timeline::ActorCategory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveFamily {
    TextLike,
    VectorShape,
    Media,
    Plot,
    Container,
    Group,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrimitiveCapabilities {
    pub text_paths: bool,
    pub vector_paths: bool,
    pub image_payload: bool,
    pub layout_container: bool,
    pub morphable_paths: bool,
    pub vector_reveal_target: bool,
    pub plot_geometry: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    pub actor_type: &'static str,
    pub family: PrimitiveFamily,
    pub capabilities: PrimitiveCapabilities,
}

impl PrimitiveDescriptor {
    pub fn for_actor_type(actor_type: &str) -> Self {
        if let Some(primitive) = find_primitive(actor_type) {
            return match primitive.type_name() {
                "Group" => Self {
                    actor_type: "Group",
                    family: PrimitiveFamily::Group,
                    capabilities: PrimitiveCapabilities::default(),
                },
                "Graph" => Self {
                    actor_type: "Graph",
                    family: PrimitiveFamily::Plot,
                    capabilities: PrimitiveCapabilities {
                        plot_geometry: true,
                        ..PrimitiveCapabilities::default()
                    },
                },
                _ => match primitive.category() {
                    ActorCategory::Shape => Self {
                        actor_type: "VectorShape",
                        family: PrimitiveFamily::VectorShape,
                        capabilities: PrimitiveCapabilities {
                            vector_paths: true,
                            morphable_paths: true,
                            vector_reveal_target: true,
                            ..PrimitiveCapabilities::default()
                        },
                    },
                    ActorCategory::Text => Self {
                        actor_type: "TextLike",
                        family: PrimitiveFamily::TextLike,
                        capabilities: PrimitiveCapabilities {
                            text_paths: true,
                            morphable_paths: true,
                            vector_reveal_target: true,
                            ..PrimitiveCapabilities::default()
                        },
                    },
                    ActorCategory::Media => {
                        if primitive.type_name() == "Svg" {
                            Self {
                                actor_type: "Svg",
                                family: PrimitiveFamily::Media,
                                capabilities: PrimitiveCapabilities {
                                    vector_paths: true,
                                    morphable_paths: true,
                                    vector_reveal_target: true,
                                    ..PrimitiveCapabilities::default()
                                },
                            }
                        } else {
                            Self {
                                actor_type: "Image",
                                family: PrimitiveFamily::Media,
                                capabilities: PrimitiveCapabilities {
                                    image_payload: true,
                                    ..PrimitiveCapabilities::default()
                                },
                            }
                        }
                    }
                    ActorCategory::Plot => Self {
                        actor_type: primitive.type_name(),
                        family: PrimitiveFamily::Plot,
                        capabilities: PrimitiveCapabilities {
                            vector_paths: true,
                            morphable_paths: true,
                            vector_reveal_target: true,
                            plot_geometry: true,
                            ..PrimitiveCapabilities::default()
                        },
                    },
                    ActorCategory::Container => Self {
                        actor_type: "Container",
                        family: PrimitiveFamily::Container,
                        capabilities: PrimitiveCapabilities {
                            layout_container: true,
                            ..PrimitiveCapabilities::default()
                        },
                    },
                },
            };
        }

        match actor_type {
            "Text" | "Math" | "Code" => Self {
                actor_type: "TextLike",
                family: PrimitiveFamily::TextLike,
                capabilities: PrimitiveCapabilities {
                    text_paths: true,
                    morphable_paths: true,
                    vector_reveal_target: true,
                    ..PrimitiveCapabilities::default()
                },
            },
            "Svg" => Self {
                actor_type: "Svg",
                family: PrimitiveFamily::Media,
                capabilities: PrimitiveCapabilities {
                    vector_paths: true,
                    morphable_paths: true,
                    vector_reveal_target: true,
                    ..PrimitiveCapabilities::default()
                },
            },
            "Image" => Self {
                actor_type: "Image",
                family: PrimitiveFamily::Media,
                capabilities: PrimitiveCapabilities {
                    image_payload: true,
                    ..PrimitiveCapabilities::default()
                },
            },
            "Graph" => Self {
                actor_type: "Graph",
                family: PrimitiveFamily::Plot,
                capabilities: PrimitiveCapabilities {
                    vector_paths: true,
                    morphable_paths: true,
                    vector_reveal_target: true,
                    plot_geometry: true,
                    ..PrimitiveCapabilities::default()
                },
            },
            "CartesianPlot" => Self::plot_curve("CartesianPlot"),
            "PolarPlot" => Self::plot_curve("PolarPlot"),
            "ParametricPlot" => Self::plot_curve("ParametricPlot"),
            "ImplicitPlot" => Self::plot_curve("ImplicitPlot"),
            "Row" | "Col" | "Grid" | "Stack" => Self {
                actor_type: "Container",
                family: PrimitiveFamily::Container,
                capabilities: PrimitiveCapabilities {
                    layout_container: true,
                    ..PrimitiveCapabilities::default()
                },
            },
            "Group" => Self {
                actor_type: "Group",
                family: PrimitiveFamily::Group,
                capabilities: PrimitiveCapabilities::default(),
            },
            _ => Self {
                actor_type: "VectorShape",
                family: PrimitiveFamily::VectorShape,
                capabilities: PrimitiveCapabilities {
                    vector_paths: true,
                    morphable_paths: true,
                    vector_reveal_target: true,
                    ..PrimitiveCapabilities::default()
                },
            },
        }
    }

    fn plot_curve(actor_type: &'static str) -> Self {
        Self {
            actor_type,
            family: PrimitiveFamily::Plot,
            capabilities: PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                plot_geometry: true,
                ..PrimitiveCapabilities::default()
            },
        }
    }

    pub fn is_plot(self) -> bool {
        self.family == PrimitiveFamily::Plot
    }

    pub fn is_graph_host(self) -> bool {
        self.actor_type == "Graph"
    }

    pub fn is_plot_curve(self) -> bool {
        self.is_plot() && !self.is_graph_host()
    }

    pub fn is_layout_container(self) -> bool {
        self.capabilities.layout_container
    }
}

#[cfg(test)]
mod tests {
    use super::{PrimitiveDescriptor, PrimitiveFamily};

    #[test]
    fn classifies_text_like_primitives() {
        let descriptor = PrimitiveDescriptor::for_actor_type("Text");
        assert_eq!(descriptor.family, PrimitiveFamily::TextLike);
        assert!(descriptor.capabilities.text_paths);
        assert!(descriptor.capabilities.morphable_paths);
    }

    #[test]
    fn classifies_plot_primitives() {
        let descriptor = PrimitiveDescriptor::for_actor_type("CartesianPlot");
        assert_eq!(descriptor.family, PrimitiveFamily::Plot);
        assert!(descriptor.capabilities.plot_geometry);
    }

    #[test]
    fn classifies_layout_containers() {
        let descriptor = PrimitiveDescriptor::for_actor_type("Row");
        assert_eq!(descriptor.family, PrimitiveFamily::Container);
        assert!(descriptor.is_layout_container());
    }

    #[test]
    fn treats_circle_as_vector_shape() {
        let descriptor = PrimitiveDescriptor::for_actor_type("Circle");
        assert_eq!(descriptor.family, PrimitiveFamily::VectorShape);
        assert!(descriptor.capabilities.vector_paths);
    }
}
