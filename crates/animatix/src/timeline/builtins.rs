use std::sync::Arc;

use super::{Environment, EvalError, Value};
use crate::easing::{Easing, apply_easing};

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

/// Deterministic splitmix64 hash — the base for every seeded stochastic
/// builtin (`seeded_rand`, the noise family), so `always` blocks stay pure
/// functions of `t` and the expression cache stays valid.
fn splitmix64(x: u64) -> u64 {
    let z = x.wrapping_add(0x9e3779b97f4a7c15);
    let z = z ^ (z >> 30);
    let z = z.wrapping_mul(0xbf58476d1ce4e5b9);
    let z = z ^ (z >> 27);
    let z = z.wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

/// Hash one integer lattice point into `[0, 1)`.
fn noise_lattice(seed: u64, x: i64, y: i64) -> f64 {
    let mixed = (x as u64).wrapping_mul(0x9e3779b97f4a7c15) ^ (y as u64).rotate_left(21);
    let h = splitmix64(seed ^ splitmix64(mixed));
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Smooth 2-D value noise in `[0, 1]`: hashed lattice corners blended with
/// smoothstep. 1-D noise is the `y = 0` slice.
fn value_noise(seed: u64, x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.floor(), y.floor());
    let (tx, ty) = (x - ix, y - iy);
    let tx = tx * tx * (3.0 - 2.0 * tx);
    let ty = ty * ty * (3.0 - 2.0 * ty);
    let (ix, iy) = (ix as i64, iy as i64);
    let n00 = noise_lattice(seed, ix, iy);
    let n10 = noise_lattice(seed, ix + 1, iy);
    let n01 = noise_lattice(seed, ix, iy + 1);
    let n11 = noise_lattice(seed, ix + 1, iy + 1);
    let top = n00 + (n10 - n00) * tx;
    let bottom = n01 + (n11 - n01) * tx;
    top + (bottom - top) * ty
}

/// Fractal Brownian motion over the value noise: up to 8 layers at doubling
/// frequency and half amplitude, normalized back to `[0, 1]`.
fn fbm(seed: u64, x: f64, y: f64, octaves: f64) -> f64 {
    let octaves = octaves.clamp(1.0, 8.0).floor() as u32;
    let (mut amp, mut freq, mut sum, mut norm) = (1.0, 1.0, 0.0, 0.0);
    for o in 0..octaves {
        sum += amp * value_noise(seed.wrapping_add(u64::from(o) * 0x9e37_79b9), x * freq, y * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// sRGB → OKLab (Björn Ottosson's constants). Channels in `[0, 1]`; alpha
/// passes through untouched.
pub(crate) fn oklab_from_srgb(c: [f64; 4]) -> [f64; 4] {
    let linear = |v: f64| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let (r, g, b) = (linear(c[0]), linear(c[1]), linear(c[2]));

    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();

    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
        c[3],
    ]
}

/// OKLab → sRGB — the inverse of [`oklab_from_srgb`].
pub(crate) fn srgb_from_oklab(lab: [f64; 4]) -> [f64; 4] {
    let l_ = lab[0] + 0.3963377774 * lab[1] + 0.2158037573 * lab[2];
    let m_ = lab[0] - 0.1055613458 * lab[1] - 0.0638541728 * lab[2];
    let s_ = lab[0] - 0.0894841775 * lab[1] - 1.2914855480 * lab[2];
    let (l, m, s) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);

    let delinear = |v: f64| {
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        }
    };
    let r = 4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s;
    let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s;
    let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s;
    [delinear(r), delinear(g), delinear(b), lab[3]]
}

/// Load standard mathematical and utility functions into the environment.
/// Read a color argument, accepting the same forms a `color:` property does.
///
/// A hex or named color string is a `Value::Str` at call time, so without this
/// `lerp_color_oklab("#30d158", "#ff2d55", t)` fails even though the identical
/// string works in a declaration.
fn color_arg(name: &str, value: &Value) -> Result<[f64; 4], EvalError> {
    match value {
        Value::Color(c) => Ok(*c),
        Value::Vec4(c) => Ok(*c),
        Value::Vec3(c) => Ok([c[0], c[1], c[2], 1.0]),
        Value::Str(text) => crate::timeline::utils::color_from_text(text)
            .map(|c| c.map(f64::from))
            .ok_or_else(|| EvalError::TypeMismatch(format!("{name}: '{text}' is not a color"))),
        other => Err(EvalError::TypeMismatch(format!("{name} expects a color, got {other:?}"))),
    }
}

/// Register the built-in expression constants and functions into `env`.
pub fn load_standard_library(env: &mut Environment) {
    env.set("PI", Value::Num(std::f64::consts::PI));
    env.set("E", Value::Num(std::f64::consts::E));
    env.set("TAU", Value::Num(std::f64::consts::TAU));
    env.set("pi", Value::Num(std::f64::consts::PI));
    env.set("tau", Value::Num(std::f64::consts::TAU));
    env.set("two_pi", Value::Num(std::f64::consts::TAU));
    env.set("e", Value::Num(std::f64::consts::E));

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
    register_num1!(env, "deg", |n| n * std::f64::consts::PI / 180.0);
    register_num1!(env, "rad", |n| n * 180.0 / std::f64::consts::PI);

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

    // Easing helper functions — each takes a progress t in [0,1] and returns eased t
    register_num1!(env, "ease_linear", |t| { apply_easing(t as f32, Easing::Linear) as f64 });
    register_num1!(env, "ease_in", |t| { apply_easing(t as f32, Easing::EaseIn) as f64 });
    register_num1!(env, "ease_out", |t| { apply_easing(t as f32, Easing::EaseOut) as f64 });
    register_num1!(env, "ease_in_out", |t| { apply_easing(t as f32, Easing::EaseInOut) as f64 });
    register_num1!(env, "bounce", |t| { apply_easing(t as f32, Easing::Bounce) as f64 });
    register_num1!(env, "elastic", |t| { apply_easing(t as f32, Easing::Elastic) as f64 });
    register_num1!(env, "back", |t| { apply_easing(t as f32, Easing::Back) as f64 });
    register_num1!(env, "expo", |t| { apply_easing(t as f32, Easing::Expo) as f64 });
    register_num1!(env, "expo_out", |t| { apply_easing(t as f32, Easing::ExpoOut) as f64 });
    register_num1!(env, "expo_in_out", |t| { apply_easing(t as f32, Easing::ExpoInOut) as f64 });
    // The default spring; `ease: spring(damping, frequency)` is where the
    // parameters live, and an `always` block can write the oscillator itself.
    register_num1!(env, "spring", |t| {
        apply_easing(
            t as f32,
            Easing::Spring {
                damping: animatix_syntax::easing::DEFAULT_SPRING[0],
                frequency: animatix_syntax::easing::DEFAULT_SPRING[1],
            },
        ) as f64
    });

    // Composable interpolation helpers
    env.set(
        "lerp_vec2",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("lerp_vec2", args, 3)?;
            let start = match &args[0] {
                Value::Vec2(v) => *v,
                _ => {
                    return Err(EvalError::TypeMismatch(
                        "lerp_vec2 expects start as Vec2".to_string(),
                    ));
                },
            };
            let end = match &args[1] {
                Value::Vec2(v) => *v,
                _ => {
                    return Err(EvalError::TypeMismatch(
                        "lerp_vec2 expects end as Vec2".to_string(),
                    ));
                },
            };
            let t = expect_num("lerp_vec2", &args[2])?;
            Ok(Value::Vec2([
                start[0] + (end[0] - start[0]) * t,
                start[1] + (end[1] - start[1]) * t,
            ]))
        })),
    );

    env.set(
        "lerp_color",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("lerp_color", args, 3)?;
            let start = match &args[0] {
                Value::Color(c) => *c,
                _ => {
                    return Err(EvalError::TypeMismatch(
                        "lerp_color expects start as Color".to_string(),
                    ));
                },
            };
            let end = match &args[1] {
                Value::Color(c) => *c,
                _ => {
                    return Err(EvalError::TypeMismatch(
                        "lerp_color expects end as Color".to_string(),
                    ));
                },
            };
            let t = expect_num("lerp_color", &args[2])?;
            Ok(Value::Color([
                start[0] + (end[0] - start[0]) * t,
                start[1] + (end[1] - start[1]) * t,
                start[2] + (end[2] - start[2]) * t,
                start[3] + (end[3] - start[3]) * t,
            ]))
        })),
    );

    // Perceptual color interpolation: raw sRGB channel lerps make midpoints
    // muddy (a red→green mix dips to dark olive); in OKLab the hue stays even
    // and lightness moves monotonically through the mix. Prefer this for
    // cross-color motion; `lerp_color` stays for backwards compatibility.
    env.set(
        "lerp_color_oklab",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("lerp_color_oklab", args, 3)?;
            let start = color_arg("lerp_color_oklab", &args[0])?;
            let end = color_arg("lerp_color_oklab", &args[1])?;
            let t = expect_num("lerp_color_oklab", &args[2])?;
            let a = oklab_from_srgb(start);
            let b = oklab_from_srgb(end);
            let mixed = [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
                a[3] + (b[3] - a[3]) * t,
            ];
            Ok(Value::Color(srgb_from_oklab(mixed).map(|v| v.clamp(0.0, 1.0))))
        })),
    );

    env.set("rand", Value::NativeFn(Arc::new(|_args, _env| Ok(Value::Num(fastrand::f64())))));

    // Σ_{k=lo}^{hi} f(k): invoke a single-parameter closure over an integer
    // range and accumulate. Requires the caller env so the closure body can
    // resolve the stdlib base (see docs/series_construction.md for why this
    // is a NativeFn rather than an eval_shared fast-path builtin).
    env.set(
        "sum_range",
        Value::NativeFn(Arc::new(|args, env| {
            expect_arg_count("sum_range", args, 3)?;
            let (params, body, captures) = match &args[0] {
                Value::Closure(params, body, captures) => (params, body, captures),
                other => {
                    return Err(EvalError::TypeMismatch(format!(
                        "sum_range expects a closure as first argument, got {:?}",
                        other
                    )));
                },
            };
            if params.len() != 1 {
                return Err(EvalError::TypeMismatch(format!(
                    "sum_range expects a single-parameter closure, got {} parameters",
                    params.len()
                )));
            }
            let lo = expect_num("sum_range", &args[1])?;
            let hi = expect_num("sum_range", &args[2])?;
            if lo < 0.0 || lo.fract() != 0.0 || hi < 0.0 || hi.fract() != 0.0 {
                return Err(EvalError::TypeMismatch(format!(
                    "sum_range bounds must be non-negative integers, got [{}, {}]",
                    lo, hi
                )));
            }
            let (lo, hi) = (lo as u64, hi as u64);
            if hi >= lo + 100_000 {
                return Err(EvalError::TypeMismatch(
                    "sum_range exceeded 100,000 iterations".to_string(),
                ));
            }

            // Build the child env once: caller base + lexical captures, then
            // rebind only the loop parameter per iteration. Also propagate
            // the caller's plot-sampling bindings (e.g. the bound plot
            // argument `x`): they live in binding slots, never in
            // CapturedEnv, so a term closure referencing the outer argument
            // would otherwise not resolve. Slots colliding with the loop
            // parameter are skipped — the per-iteration `set` must win.
            let mut child_env = if let Some(base) = env.base.as_ref() {
                Environment::with_base(std::sync::Arc::clone(base))
            } else {
                Environment::new()
            };
            captures.merge_into(&mut child_env);
            let param = params[0].clone();
            for (dst, src) in child_env.bindings.iter_mut().zip(env.bindings.iter()) {
                if let Some((name, _)) = src {
                    if *name == param {
                        continue;
                    }
                }
                *dst = src.clone();
            }

            let mut acc = 0.0_f64;
            for k in lo..=hi {
                child_env.set(&param, Value::Num(k as f64));
                acc += crate::timeline::modifier_runtime::ir::evaluate_compiled_expr(
                    body, &child_env,
                )?
                .as_num();
            }
            Ok(Value::Num(acc))
        })),
    );

    // Property-state query: is the referenced property currently between
    // keyframes? Takes a property reference (`&actor.prop`) produced by the
    // `&` operator. Reads the per-frame flag injected under the internal
    // animating_flag key (see env_keys::animating_flag).
    env.set(
        "is_animating",
        Value::NativeFn(Arc::new(|args, env| {
            expect_arg_count("is_animating", args, 1)?;
            let (label, prop) = match &args[0] {
                Value::PropRef { label, prop } => (label, prop),
                other => {
                    return Err(EvalError::TypeMismatch(format!(
                        "is_animating expects a property reference (&actor.prop), got {:?}",
                        other
                    )));
                },
            };
            let flag_key = crate::timeline::env_keys::animating_flag(label, prop);
            let animating = match env.get(&flag_key) {
                Some(Value::Bool(b)) => b,
                Some(Value::Num(n)) => n != 0.0,
                _ => false,
            };
            Ok(Value::Bool(animating))
        })),
    );

    // Deterministic pseudo-random using splitmix64 hash.
    // Same seed always produces the same value in [0, 1).
    env.set(
        "seeded_rand",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("seeded_rand", args, 1)?;
            let seed = expect_num("seeded_rand", &args[0])?;
            let hash = splitmix64(seed.to_bits());
            Ok(Value::Num(hash as f64 / u64::MAX as f64))
        })),
    );

    // ── organic motion: seeded value noise ───────────────────────────────
    // Pure functions of their arguments (the expression-cache requirement),
    // so an `always` block can drive wobble / drift / flicker as
    // `noise(t * k)` or layer octaves through `fbm` for organic irregularity.
    const DEFAULT_NOISE_SEED: u64 = 0x006e_6f69_7365;
    register_num1!(env, "noise", |x| value_noise(DEFAULT_NOISE_SEED, x, 0.0));
    register_num2!(env, "noise2", |x: f64, y: f64| value_noise(DEFAULT_NOISE_SEED, x, y));
    register_num2!(env, "seeded_noise", |seed: f64, x: f64| value_noise(seed.to_bits(), x, 0.0));
    register_num3!(env, "seeded_noise2", |seed: f64, x: f64, y: f64| value_noise(
        seed.to_bits(),
        x,
        y
    ));
    register_num2!(env, "fbm", |x: f64, octaves: f64| fbm(DEFAULT_NOISE_SEED, x, 0.0, octaves));
    register_num3!(env, "seeded_fbm", |seed: f64, x: f64, octaves: f64| fbm(
        seed.to_bits(),
        x,
        0.0,
        octaves
    ));

    for name in ["RED", "GREEN", "BLUE", "BLACK", "WHITE"] {
        if let Some(color) = animatix_syntax::typing::named_color_rgba(name) {
            env.set(name, Value::Color(color));
        }
    }

    env.set(
        "rgb",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("rgb", args, 3)?;
            match (&args[0], &args[1], &args[2]) {
                (Value::Num(r), Value::Num(g), Value::Num(b)) => {
                    Ok(Value::Color([*r / 255.0, *g / 255.0, *b / 255.0, 1.0]))
                },
                _ => Err(EvalError::TypeMismatch("rgb expects 3 numbers".to_string())),
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
                },
                _ => Err(EvalError::TypeMismatch("rgba expects 4 numbers".to_string())),
            }
        })),
    );

    env.set(
        "vec2",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("vec2", args, 2)?;
            Ok(Value::Vec2([expect_num("vec2", &args[0])?, expect_num("vec2", &args[1])?]))
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
