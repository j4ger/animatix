#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bounce,
    Elastic,
    Back,
    Expo,
}

pub const EASING_REGISTRY: &[(&str, &str)] = &[
    ("linear", "Linear"),
    ("easein", "Ease In"),
    ("easeout", "Ease Out"),
    ("easeinout", "Ease In Out"),
    ("bounce", "Bounce"),
    ("elastic", "Elastic"),
    ("back", "Back"),
    ("expo", "Expo"),
];

pub fn apply_easing(progress: f32, easing: Easing) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => t * (2.0 - t),
        Easing::EaseInOut => {
            if t < 0.5 {
                2.0 * t * t
            } else {
                -1.0 + (4.0 - 2.0 * t) * t
            }
        }
        Easing::Bounce => {
            let n1 = 7.5625;
            let d1 = 2.75;
            if t < 1.0 / d1 {
                n1 * t * t
            } else if t < 2.0 / d1 {
                let t = t - 1.5 / d1;
                n1 * t * t + 0.75
            } else if t < 2.5 / d1 {
                let t = t - 2.25 / d1;
                n1 * t * t + 0.9375
            } else {
                let t = t - 2.625 / d1;
                n1 * t * t + 0.984375
            }
        }
        Easing::Elastic => {
            if t == 0.0 || t == 1.0 {
                return t;
            }
            let c4 = (2.0 * std::f32::consts::PI) / 3.0;
            -(2.0_f32.powf(10.0 * (t - 1.0))) * ((t * 10.0 - 10.75) * c4).sin()
        }
        Easing::Back => {
            let c1 = 1.70158;
            let c3 = c1 + 1.0;
            c3 * t * t * t - c1 * t * t
        }
        Easing::Expo => {
            if t == 0.0 {
                0.0
            } else {
                2.0_f32.powf(10.0 * (t - 1.0))
            }
        }
    }
}

pub fn parse_easing_name(raw: &str) -> Option<Easing> {
    match raw {
        "ease-in" | "easein" => Some(Easing::EaseIn),
        "ease-out" | "easeout" => Some(Easing::EaseOut),
        "ease-in-out" | "easeinout" => Some(Easing::EaseInOut),
        "bounce" => Some(Easing::Bounce),
        "elastic" => Some(Easing::Elastic),
        "back" => Some(Easing::Back),
        "expo" => Some(Easing::Expo),
        "linear" => Some(Easing::Linear),
        _ => None,
    }
}
