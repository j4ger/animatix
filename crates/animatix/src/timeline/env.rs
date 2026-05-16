use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum EvalError {
    UndefinedVariable(String),
    TypeMismatch(String),
    NotCallable(String),
    UnsupportedMethod(String),
    UnsupportedIndex,
    UnsupportedConstruct(String),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UndefinedVariable(v) => write!(f, "Undefined variable: {}", v),
            EvalError::TypeMismatch(e) => write!(f, "Type mismatch: {}", e),
            EvalError::NotCallable(n) => write!(f, "Not callable: {}", n),
            EvalError::UnsupportedMethod(name) => {
                write!(f, "Unsupported runtime method call: {}", name)
            }
            EvalError::UnsupportedIndex => {
                write!(f, "Unsupported runtime index expression")
            }
            EvalError::UnsupportedConstruct(name) => {
                write!(f, "Unsupported runtime construct expression: {}", name)
            }
        }
    }
}

impl std::error::Error for EvalError {}

#[derive(Clone)]
pub enum Value {
    Num(f64),
    Str(String),
    Bool(bool),
    Vec2([f64; 2]),
    Vec3([f64; 3]),
    Vec4([f64; 4]),
    Color([f64; 4]),
    List(Vec<Value>),
    /// Object(type_name, fields) — constructed value with named fields
    Object(String, HashMap<String, Value>),
    NativeFn(Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>),
    Closure(Vec<String>, Box<crate::ast::Expr>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Num(n) => write!(f, "Num({})", n),
            Value::Str(s) => write!(f, "Str({:?})", s),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::Vec2(v) => write!(f, "Vec2({:?})", v),
            Value::Vec3(v) => write!(f, "Vec3({:?})", v),
            Value::Vec4(v) => write!(f, "Vec4({:?})", v),
            Value::Color(c) => write!(f, "Color({:?})", c),
            Value::List(items) => write!(f, "List({:?})", items),
            Value::Object(name, fields) => write!(f, "{}({:?})", name, fields),
            Value::NativeFn(_) => write!(f, "<NativeFn>"),
            Value::Closure(args, _) => write!(f, "<Closure({:?})>", args),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Num(a), Value::Num(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Vec2(a), Value::Vec2(b)) => a == b,
            (Value::Vec3(a), Value::Vec3(b)) => a == b,
            (Value::Vec4(a), Value::Vec4(b)) => a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Object(name_a, fields_a), Value::Object(name_b, fields_b)) => {
                name_a == name_b && fields_a == fields_b
            }
            // Native functions and closures cannot be compared for equality
            _ => false,
        }
    }
}

impl Value {
    pub fn as_num(&self) -> f64 {
        match self {
            Value::Num(n) => *n,
            _ => 0.0,
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            _ => false,
        }
    }

    pub fn as_vec2(&self) -> [f64; 2] {
        match self {
            Value::Vec2(v) => *v,
            _ => [0.0, 0.0],
        }
    }

    pub fn as_vec3(&self) -> [f64; 3] {
        match self {
            Value::Vec3(v) => *v,
            _ => [0.0, 0.0, 0.0],
        }
    }

    pub fn as_vec4(&self) -> [f64; 4] {
        match self {
            Value::Vec4(v) => *v,
            _ => [0.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn as_color(&self) -> [f64; 4] {
        match self {
            Value::Color(c) => *c,
            _ => [0.0, 0.0, 0.0, 1.0],
        }
    }

    pub fn as_list(&self) -> Vec<Value> {
        match self {
            Value::List(items) => items.clone(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct Environment {
    values: HashMap<String, Value>,
}

impl Environment {
    pub fn new() -> Self {
        Environment {
            values: HashMap::new(),
        }
    }

    pub fn set(&mut self, name: &str, value: Value) {
        self.values.insert(name.to_string(), value);
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        self.values.get(name).cloned()
    }

    pub fn all_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.values.keys().cloned().collect();
        keys.sort();
        keys
    }
}

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

pub fn load_standard_library(env: &mut Environment) {
    env.set("PI", Value::Num(std::f64::consts::PI));
    env.set("E", Value::Num(std::f64::consts::E));
    env.set("TAU", Value::Num(std::f64::consts::TAU));

    env.set(
        "sin",
        Value::NativeFn(Arc::new(|args, _env| {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "sin expects 1 argument".to_string(),
                ));
            }
            if let Value::Num(n) = &args[0] {
                Ok(Value::Num(n.sin()))
            } else {
                Err(EvalError::TypeMismatch("sin expects a number".to_string()))
            }
        })),
    );

    env.set(
        "cos",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("cos", args, 1)?;
            Ok(Value::Num(expect_num("cos", &args[0])?.cos()))
        })),
    );

    env.set(
        "tan",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("tan", args, 1)?;
            Ok(Value::Num(expect_num("tan", &args[0])?.tan()))
        })),
    );

    env.set(
        "asin",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("asin", args, 1)?;
            Ok(Value::Num(expect_num("asin", &args[0])?.asin()))
        })),
    );

    env.set(
        "acos",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("acos", args, 1)?;
            Ok(Value::Num(expect_num("acos", &args[0])?.acos()))
        })),
    );

    env.set(
        "atan",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("atan", args, 1)?;
            Ok(Value::Num(expect_num("atan", &args[0])?.atan()))
        })),
    );

    env.set(
        "abs",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("abs", args, 1)?;
            Ok(Value::Num(expect_num("abs", &args[0])?.abs()))
        })),
    );

    env.set(
        "floor",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("floor", args, 1)?;
            Ok(Value::Num(expect_num("floor", &args[0])?.floor()))
        })),
    );

    env.set(
        "ceil",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("ceil", args, 1)?;
            Ok(Value::Num(expect_num("ceil", &args[0])?.ceil()))
        })),
    );

    env.set(
        "round",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("round", args, 1)?;
            Ok(Value::Num(expect_num("round", &args[0])?.round()))
        })),
    );

    env.set(
        "sqrt",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("sqrt", args, 1)?;
            Ok(Value::Num(expect_num("sqrt", &args[0])?.sqrt()))
        })),
    );

    env.set(
        "exp",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("exp", args, 1)?;
            Ok(Value::Num(expect_num("exp", &args[0])?.exp()))
        })),
    );

    env.set(
        "ln",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("ln", args, 1)?;
            Ok(Value::Num(expect_num("ln", &args[0])?.ln()))
        })),
    );

    env.set(
        "log10",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("log10", args, 1)?;
            Ok(Value::Num(expect_num("log10", &args[0])?.log10()))
        })),
    );

    env.set(
        "signum",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("signum", args, 1)?;
            Ok(Value::Num(expect_num("signum", &args[0])?.signum()))
        })),
    );

    env.set(
        "fract",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("fract", args, 1)?;
            Ok(Value::Num(expect_num("fract", &args[0])?.fract()))
        })),
    );

    env.set(
        "deg_to_rad",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("deg_to_rad", args, 1)?;
            let n = expect_num("deg_to_rad", &args[0])?;
            Ok(Value::Num(n * std::f64::consts::PI / 180.0))
        })),
    );

    env.set(
        "rad_to_deg",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("rad_to_deg", args, 1)?;
            let n = expect_num("rad_to_deg", &args[0])?;
            Ok(Value::Num(n * 180.0 / std::f64::consts::PI))
        })),
    );

    env.set(
        "min",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("min", args, 2)?;
            Ok(Value::Num(
                expect_num("min", &args[0])?.min(expect_num("min", &args[1])?),
            ))
        })),
    );

    env.set(
        "max",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("max", args, 2)?;
            Ok(Value::Num(
                expect_num("max", &args[0])?.max(expect_num("max", &args[1])?),
            ))
        })),
    );

    env.set(
        "pow",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("pow", args, 2)?;
            Ok(Value::Num(
                expect_num("pow", &args[0])?.powf(expect_num("pow", &args[1])?),
            ))
        })),
    );

    env.set(
        "atan2",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("atan2", args, 2)?;
            Ok(Value::Num(
                expect_num("atan2", &args[0])?.atan2(expect_num("atan2", &args[1])?),
            ))
        })),
    );

    env.set(
        "hypot",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("hypot", args, 2)?;
            Ok(Value::Num(
                expect_num("hypot", &args[0])?.hypot(expect_num("hypot", &args[1])?),
            ))
        })),
    );

    env.set(
        "rem",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("rem", args, 2)?;
            Ok(Value::Num(expect_num("rem", &args[0])? % expect_num("rem", &args[1])?))
        })),
    );

    env.set(
        "clamp",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("clamp", args, 3)?;
            let val = expect_num("clamp", &args[0])?;
            let min = expect_num("clamp", &args[1])?;
            let max = expect_num("clamp", &args[2])?;
            Ok(Value::Num(val.clamp(min, max)))
        })),
    );

    env.set(
        "smoothstep",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("smoothstep", args, 3)?;
            let edge0 = expect_num("smoothstep", &args[0])?;
            let edge1 = expect_num("smoothstep", &args[1])?;
            let x = expect_num("smoothstep", &args[2])?;
            let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
            Ok(Value::Num(t * t * (3.0 - 2.0 * t)))
        })),
    );

    env.set(
        "step",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("step", args, 2)?;
            let edge = expect_num("step", &args[0])?;
            let x = expect_num("step", &args[1])?;
            Ok(Value::Num(if x < edge { 0.0 } else { 1.0 }))
        })),
    );

    env.set(
        "lerp",
        Value::NativeFn(Arc::new(|args, _env| {
            expect_arg_count("lerp", args, 3)?;
            match (&args[0], &args[1], &args[2]) {
                (Value::Num(start), Value::Num(end), Value::Num(t)) => {
                    Ok(Value::Num(start + (end - start) * t))
                }
                _ => Err(EvalError::TypeMismatch(
                    "lerp expects 3 numbers".to_string(),
                )),
            }
        })),
    );

    env.set(
        "rand",
        Value::NativeFn(Arc::new(|_args, _env| Ok(Value::Num(rand::random::<f64>())))),
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
