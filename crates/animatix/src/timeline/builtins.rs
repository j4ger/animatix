use std::sync::Arc;

use super::{Environment, EvalError, Value};

fn expect_arg_count(name: &str, args: &[Value], expected: usize) -> Result<(), EvalError> {
    if args.len() != expected {
        return Err(EvalError::TypeMismatch(format!(
            "{} expects {} argument{}",
            name,
            expected,
            if expected == 1 { "" } else { "s" }
        )));
    }
    Ok(())
}

fn expect_num(name: &str, value: &Value) -> Result<f64, EvalError> {
    match value {
        Value::Num(n) => Ok(*n),
        _ => Err(EvalError::TypeMismatch(format!("{} expects a number", name))),
    }
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [f64; 3] {
    if s <= 0.0 {
        return [v, v, v];
    }

    let h = h.rem_euclid(360.0) / 60.0;
    let i = h.floor() as i32;
    let f = h - i as f64;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));

    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s <= 0.0 {
        return [l, l, l];
    }

    let h = h.rem_euclid(360.0) / 360.0;

    fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;

    [
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    ]
}

#[allow(unused_macros)]
macro_rules! register_num1 {
    ($env:expr, $name:literal, $f:expr) => {
        $env.set(
            $name,
            Value::NativeFn(Arc::new(|args, _env| {
                expect_arg_count($name, args, 1)?;
                Ok(Value::Num($f(expect_num($name, &args[0])?)))
            })),
        );
    };
}

#[allow(unused_macros)]
macro_rules! register_num2 {
    ($env:expr, $name:literal, $f:expr) => {
        $env.set(
            $name,
            Value::NativeFn(Arc::new(|args, _env| {
                expect_arg_count($name, args, 2)?;
                let a = expect_num($name, &args[0])?;
                let b = expect_num($name, &args[1])?;
                Ok(Value::Num($f(a, b)))
            })),
        );
    };
}

#[allow(unused_macros)]
macro_rules! register_num3 {
    ($env:expr, $name:literal, $f:expr) => {
        $env.set(
            $name,
            Value::NativeFn(Arc::new(|args, _env| {
                expect_arg_count($name, args, 3)?;
                let a = expect_num($name, &args[0])?;
                let b = expect_num($name, &args[1])?;
                let c = expect_num($name, &args[2])?;
                Ok(Value::Num($f(a, b, c)))
            })),
        );
    };
}

/// Load standard mathematical and utility functions into the environment.
pub fn load_standard_library(env: &mut Environment) {
    env.set("PI", Value::Num(std::f64::consts::PI));
    env.set("E", Value::Num(std::f64::consts::E));
    env.set("TAU", Value::Num(std::f64::consts::TAU));

    register_num1!(env, "sin", f64::sin);
    register_num1!(env, "cos", f64::cos);
    register_num1!(env, "tan", f64::tan);
    register_num1!(env, "asin", f64::asin);
    register_num1!(env, "acos", f64::acos);
    register_num1!(env, "atan", f64::atan);
    register_num1!(env, "abs", f64::abs);
    register_num1!(env, "floor", f64::floor);
    register_num1!(env, "ceil", f64::ceil);
    register_num1!(env, "round", f64::round);
    register_num1!(env, "sqrt", f64::sqrt);
    register_num1!(env, "exp", f64::exp);
    register_num1!(env, "ln", f64::ln);
    register_num1!(env, "log10", f64::log10);
    register_num1!(env, "signum", f64::signum);
    register_num1!(env, "fract", f64::fract);
    register_num1!(env, "deg_to_rad", |n| n * std::f64::consts::PI / 180.0);
    register_num1!(env, "rad_to_deg", |n| n * 180.0 / std::f64::consts::PI);

    register_num2!(env, "min", f64::min);
    register_num2!(env, "max", f64::max);
    register_num2!(env, "pow", f64::powf);
    register_num2!(env, "atan2", f64::atan2);
    register_num2!(env, "hypot", f64::hypot);
    register_num2!(env, "rem", |a, b| a % b);
    register_num2!(env, "step", |edge, x| if x < edge { 0.0 } else { 1.0 });

    register_num3!(env, "clamp", |val: f64, min: f64, max: f64| val.clamp(min, max));
    register_num3!(env, "smoothstep", |edge0: f64, edge1: f64, x: f64| {
        let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    });
    register_num3!(env, "lerp", |start, end, t| start + (end - start) * t);

    env.set(
        "rand",
        Value::NativeFn(Arc::new(|_args, _env| Ok(Value::Num(fastrand::f64())))),
    );

    // Deterministic pseudo-random using splitmix64 hash.
    // Same seed always produces the same value in [0, 1).
    fn splitmix64(x: u64) -> u64 {
        let z = x.wrapping_add(0x9e3779b97f4a7c15);
        let z = z ^ (z >> 30);
        let z = z.wrapping_mul(0xbf58476d1ce4e5b9);
        let z = z ^ (z >> 27);
        let z = z.wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    env.set(
        "seeded_rand",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("seeded_rand", args, 1)?;
            let seed = expect_num("seeded_rand", &args[0])?;
            let hash = splitmix64(seed.to_bits());
            Ok(Value::Num(hash as f64 / u64::MAX as f64))
        })),
    );

    env.set("RED", Value::Color([1.0, 0.0, 0.0, 1.0]));
    env.set("GREEN", Value::Color([0.0, 1.0, 0.0, 1.0]));
    env.set("BLUE", Value::Color([0.0, 0.0, 1.0, 1.0]));
    env.set("BLACK", Value::Color([0.0, 0.0, 0.0, 1.0]));
    env.set("WHITE", Value::Color([1.0, 1.0, 1.0, 1.0]));

    env.set(
        "rgb",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("rgb", args, 3)?;
            match (&args[0], &args[1], &args[2]) {
                (Value::Num(r), Value::Num(g), Value::Num(b)) => {
                    Ok(Value::Color([*r / 255.0, *g / 255.0, *b / 255.0, 1.0]))
                }
                _ => Err(EvalError::TypeMismatch(
                    "rgb expects 3 numbers".to_string(),
                )),
            }
        })),
    );

    env.set(
        "rgba",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("rgba", args, 4)?;
            match (&args[0], &args[1], &args[2], &args[3]) {
                (Value::Num(r), Value::Num(g), Value::Num(b), Value::Num(a)) => {
                    Ok(Value::Color([*r, *g, *b, *a]))
                }
                _ => Err(EvalError::TypeMismatch(
                    "rgba expects 4 numbers".to_string(),
                )),
            }
        })),
    );

    env.set(
        "vec2",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("vec2", args, 2)?;
            Ok(Value::Vec2([
                expect_num("vec2", &args[0])?,
                expect_num("vec2", &args[1])?,
            ]))
        })),
    );

    env.set(
        "vec3",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("vec3", args, 3)?;
            Ok(Value::Vec3([
                expect_num("vec3", &args[0])?,
                expect_num("vec3", &args[1])?,
                expect_num("vec3", &args[2])?,
            ]))
        })),
    );

    env.set(
        "vec4",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("vec4", args, 4)?;
            Ok(Value::Vec4([
                expect_num("vec4", &args[0])?,
                expect_num("vec4", &args[1])?,
                expect_num("vec4", &args[2])?,
                expect_num("vec4", &args[3])?,
            ]))
        })),
    );

    env.set(
        "hsv",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("hsv", args, 3)?;
            let h = expect_num("hsv", &args[0])?;
            let s = expect_num("hsv", &args[1])?;
            let v = expect_num("hsv", &args[2])?;
            let [r, g, b] = hsv_to_rgb(h, s, v);
            Ok(Value::Color([r, g, b, 1.0]))
        })),
    );

    env.set(
        "hsla",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("hsla", args, 4)?;
            let h = expect_num("hsla", &args[0])?;
            let s = expect_num("hsla", &args[1])?;
            let l = expect_num("hsla", &args[2])?;
            let a = expect_num("hsla", &args[3])?;
            let [r, g, b] = hsl_to_rgb(h, s, l);
            Ok(Value::Color([r, g, b, a]))
        })),
    );
}
