use crate::ast::{Expr, Time};

pub fn parse_color(expr: &Expr) -> [f32; 4] {
    if let Expr::Ident(name) = expr {
        match name.as_str() {
            "red" => [1.0, 0.0, 0.0, 1.0],
            "green" => [0.0, 1.0, 0.0, 1.0],
            "blue" => [0.0, 0.0, 1.0, 1.0],
            "black" => [0.0, 0.0, 0.0, 1.0],
            "white" => [1.0, 1.0, 1.0, 1.0],
            "yellow" => [1.0, 1.0, 0.0, 1.0],
            _ => [0.8, 0.8, 0.8, 1.0],
        }
    } else {
        [0.8, 0.8, 0.8, 1.0]
    }
}

pub fn time_to_ms(time: &Time) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
    }
}
