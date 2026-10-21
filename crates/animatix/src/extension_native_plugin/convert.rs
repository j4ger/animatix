//! Value conversions and arena allocation between host engine types and plugin ABI types.

use std::ffi::{CStr, c_char};

use animatix_plugin_api::{
    NATIVE_CAP_IMAGE_PAYLOAD, NATIVE_CAP_IS_CONTAINER, NATIVE_CAP_IS_SHAPE,
    NATIVE_CAP_LAYOUT_CONTAINER, NATIVE_CAP_PLOT_GEOMETRY, NATIVE_CAP_PLOT_HOST,
    NATIVE_CAP_TEXT_PATHS, NATIVE_CAP_VECTOR_PATHS, NATIVE_EASING_BACK, NATIVE_EASING_BOUNCE_IN,
    NATIVE_EASING_ELASTIC, NATIVE_EASING_EXPO, NATIVE_EASING_EXPO_IN_OUT, NATIVE_EASING_EXPO_OUT,
    NATIVE_EASING_IN, NATIVE_EASING_IN_OUT, NATIVE_EASING_LINEAR, NATIVE_EASING_OUT,
    NATIVE_EASING_UNSUPPORTED, NATIVE_PATH_ARC, NATIVE_PATH_CUBIC, NATIVE_PATH_ELLIPSE,
    NATIVE_PATH_LINE, NATIVE_PATH_POLYGON, NATIVE_PATH_QUADRATIC, NATIVE_PATH_RECT,
    NATIVE_PATH_ROUNDED_RECT, NATIVE_PROPERTY_BOOL, NATIVE_PROPERTY_ENUM, NATIVE_PROPERTY_F32,
    NATIVE_PROPERTY_GENERIC, NATIVE_PROPERTY_POINT_LIST, NATIVE_PROPERTY_STRING,
    NATIVE_PROPERTY_U32, NATIVE_PROPERTY_VEC2, NATIVE_PROPERTY_VEC4, NATIVE_RESIZE_MODE_SCALE,
    NATIVE_VALUE_BOOL, NATIVE_VALUE_COLOR, NATIVE_VALUE_COMMAND_LIST, NATIVE_VALUE_ENUM,
    NATIVE_VALUE_LIST, NATIVE_VALUE_NUM, NATIVE_VALUE_POINT_LIST, NATIVE_VALUE_STRING,
    NATIVE_VALUE_STRING_LIST, NATIVE_VALUE_TRANSFORM, NATIVE_VALUE_U32, NATIVE_VALUE_VARIANT,
    NATIVE_VALUE_VEC2, NATIVE_VALUE_VEC3, NATIVE_VALUE_VEC4, NativePathCommand, NativeValue,
};
use kurbo::Shape;

use crate::primitives::ChildProcessing;
use crate::timeline::{ActorCategory, EvalError, PropertyValue, ResizeMode, Value};

pub(crate) unsafe fn read_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
}

pub(crate) unsafe fn read_c_string_len(ptr: *const c_char, len: usize) -> Option<String> {
    if ptr.is_null() {
        return Some(String::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

/// Map a `NativeHighlightCommand.blend` / equation-fragment blend code.
pub(crate) fn native_blend_from_code(code: u32) -> vello::peniko::Mix {
    match code {
        1 => vello::peniko::Mix::Multiply,
        2 => vello::peniko::Mix::Difference,
        3 => vello::peniko::Mix::Screen,
        _ => vello::peniko::Mix::Normal,
    }
}

pub(crate) fn native_color([r, g, b, a]: [f64; 4]) -> vello::peniko::Color {
    let clamp = |component: f64| (component.clamp(0.0, 1.0) * 255.0).round() as u8;
    vello::peniko::Color::from_rgba8(clamp(r), clamp(g), clamp(b), clamp(a))
}

pub(crate) fn native_path_to_vello(
    command: &NativePathCommand,
) -> Option<crate::timeline::VelloPath> {
    let mut path = kurbo::BezPath::new();
    match command.kind {
        NATIVE_PATH_RECT => {
            let rect = kurbo::Rect::new(
                command.x,
                command.y,
                command.x + command.width,
                command.y + command.height,
            );
            path = rect.into_path(1e-3);
        },
        NATIVE_PATH_ELLIPSE => {
            let ellipse =
                kurbo::Ellipse::new((command.x, command.y), (command.width, command.height), 0.0);
            path = ellipse.into_path(1e-3);
        },
        NATIVE_PATH_LINE => {
            path.move_to((command.x1, command.y1));
            path.line_to((command.x2, command.y2));
        },
        NATIVE_PATH_POLYGON => {
            if command.points.is_null() || command.point_len == 0 {
                return None;
            }
            // The callback owns the points buffer for the duration of the call,
            // and this conversion copies them into a host-owned BezPath.
            let points = unsafe { std::slice::from_raw_parts(command.points, command.point_len) };
            path.move_to((points[0].x, points[0].y));
            for point in points.iter().skip(1) {
                path.line_to((point.x, point.y));
            }
            path.close_path();
        },
        NATIVE_PATH_CUBIC => {
            path.move_to((command.x, command.y));
            path.curve_to(
                (command.x1, command.y1),
                (command.x2, command.y2),
                (command.width, command.height),
            );
        },
        NATIVE_PATH_QUADRATIC => {
            path.move_to((command.x, command.y));
            path.quad_to((command.x1, command.y1), (command.x2, command.y2));
        },
        NATIVE_PATH_ARC => {
            let arc = kurbo::Arc::new(
                (command.x, command.y),
                (command.width, command.height),
                command.start_angle,
                command.sweep_angle,
                0.0,
            );
            path = arc.into_path(1e-3);
        },
        NATIVE_PATH_ROUNDED_RECT => {
            let rect = kurbo::RoundedRect::new(
                command.x,
                command.y,
                command.x + command.width,
                command.y + command.height,
                command.radius,
            );
            path = rect.into_path(1e-3);
        },
        _ => return None,
    }
    let fill = native_color(command.fill);
    let stroke = if command.stroke_width > 0.0 {
        Some((native_color(command.stroke), command.stroke_width as f32))
    } else {
        None
    };
    Some(crate::timeline::VelloPath {
        path: std::sync::Arc::new(path),
        fill: Some(fill),
        stroke,
        line_cap: command.line_cap,
        line_join: command.line_join,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    })
}

pub(crate) fn native_primitive_category(category: u32) -> Option<ActorCategory> {
    use animatix_plugin_api::{
        NATIVE_PRIMITIVE_CATEGORY_ANNOTATION, NATIVE_PRIMITIVE_CATEGORY_CONTAINER,
        NATIVE_PRIMITIVE_CATEGORY_MEDIA, NATIVE_PRIMITIVE_CATEGORY_PLOT,
        NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CATEGORY_TEXT,
    };
    match category {
        NATIVE_PRIMITIVE_CATEGORY_SHAPE => Some(ActorCategory::Shape),
        NATIVE_PRIMITIVE_CATEGORY_TEXT => Some(ActorCategory::Text),
        NATIVE_PRIMITIVE_CATEGORY_MEDIA => Some(ActorCategory::Media),
        NATIVE_PRIMITIVE_CATEGORY_PLOT => Some(ActorCategory::Plot),
        NATIVE_PRIMITIVE_CATEGORY_CONTAINER => Some(ActorCategory::Container),
        NATIVE_PRIMITIVE_CATEGORY_ANNOTATION => Some(ActorCategory::Annotation),
        _ => None,
    }
}

pub(crate) fn native_child_processing(kind: u32) -> Option<ChildProcessing> {
    use animatix_plugin_api::{
        NATIVE_PRIMITIVE_CHILD_EQUATION, NATIVE_PRIMITIVE_CHILD_FILTER,
        NATIVE_PRIMITIVE_CHILD_GENERIC, NATIVE_PRIMITIVE_CHILD_MASK,
    };
    match kind {
        NATIVE_PRIMITIVE_CHILD_GENERIC => Some(ChildProcessing::Generic),
        NATIVE_PRIMITIVE_CHILD_FILTER => Some(ChildProcessing::Filter),
        NATIVE_PRIMITIVE_CHILD_MASK => Some(ChildProcessing::Mask),
        NATIVE_PRIMITIVE_CHILD_EQUATION => Some(ChildProcessing::Equation),
        _ => None,
    }
}

pub(crate) fn native_capabilities(flags: u32) -> animatix_syntax::schema::PrimitiveCapabilities {
    let is_container = flags & NATIVE_CAP_IS_CONTAINER != 0;
    let has_visual_content = !is_container
        && (flags
            & (NATIVE_CAP_IS_SHAPE
                | NATIVE_CAP_TEXT_PATHS
                | NATIVE_CAP_IMAGE_PAYLOAD
                | NATIVE_CAP_VECTOR_PATHS
                | NATIVE_CAP_PLOT_GEOMETRY
                | NATIVE_CAP_PLOT_HOST)
            != 0);
    animatix_syntax::schema::PrimitiveCapabilities {
        text_paths: flags & NATIVE_CAP_TEXT_PATHS != 0,
        vector_paths: flags & NATIVE_CAP_VECTOR_PATHS != 0,
        image_payload: flags & NATIVE_CAP_IMAGE_PAYLOAD != 0,
        layout_container: flags & NATIVE_CAP_LAYOUT_CONTAINER != 0,
        plot_geometry: flags & NATIVE_CAP_PLOT_GEOMETRY != 0,
        plot_host: flags & NATIVE_CAP_PLOT_HOST != 0,
        is_container,
        is_shape: flags & NATIVE_CAP_IS_SHAPE != 0,
        has_visual_content,
        stroke_primary: false,
        callout: false,
        legend_host: false,
        time_dependent: false,
    }
}

pub(crate) fn native_resize_mode(mode: u32) -> ResizeMode {
    if mode == NATIVE_RESIZE_MODE_SCALE {
        ResizeMode::Scale
    } else {
        ResizeMode::Size
    }
}

pub(crate) fn parse_native_type(ty: &str) -> Option<animatix_syntax::typing::Type> {
    // Strict annotation parsing: `None` for unknown/unsupported type strings.
    animatix_syntax::typing::Type::parse(ty)
}

pub(crate) fn native_property_kind(
    kind: u32,
) -> Option<animatix_syntax::schema::PropertyValueKind> {
    match kind {
        NATIVE_PROPERTY_F32 => Some(animatix_syntax::schema::PropertyValueKind::F32),
        NATIVE_PROPERTY_U32 => Some(animatix_syntax::schema::PropertyValueKind::U32),
        NATIVE_PROPERTY_BOOL => Some(animatix_syntax::schema::PropertyValueKind::Bool),
        NATIVE_PROPERTY_VEC2 => Some(animatix_syntax::schema::PropertyValueKind::Vec2),
        NATIVE_PROPERTY_VEC4 => Some(animatix_syntax::schema::PropertyValueKind::Vec4),
        NATIVE_PROPERTY_STRING => Some(animatix_syntax::schema::PropertyValueKind::String),
        NATIVE_PROPERTY_POINT_LIST => Some(animatix_syntax::schema::PropertyValueKind::PointList),
        NATIVE_PROPERTY_GENERIC | NATIVE_PROPERTY_ENUM => {
            Some(animatix_syntax::schema::PropertyValueKind::Generic)
        },
        _ => None,
    }
}

#[derive(Default)]
pub(crate) struct NativeValueArena {
    strings: Vec<Vec<u8>>,
    lists: Vec<Vec<NativeValue>>,
    values: Vec<Vec<NativeValue>>,
}

impl NativeValueArena {
    pub(crate) fn string(&mut self, text: &str) -> (*const c_char, usize) {
        let len = text.len();
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let ptr = bytes.as_ptr().cast::<c_char>();
        self.strings.push(bytes);
        (ptr, len)
    }

    pub(crate) fn list(&mut self, values: Vec<NativeValue>) -> (*const NativeValue, usize) {
        let ptr = values.as_ptr();
        let len = values.len();
        self.lists.push(values);
        (ptr, len)
    }

    pub(crate) fn value(&mut self, value: NativeValue) -> *const NativeValue {
        self.values.push(vec![value]);
        self.values.last().expect("payload just pushed").as_ptr()
    }
}

pub(crate) fn native_string(tag: u32, text: &str, arena: &mut NativeValueArena) -> NativeValue {
    let (string, string_len) = arena.string(text);
    NativeValue {
        tag,
        string,
        string_len,
        ..NativeValue::default()
    }
}

pub(crate) fn value_to_native(
    value: &Value,
    arena: &mut NativeValueArena,
) -> Result<NativeValue, EvalError> {
    let mut native = NativeValue::default();
    match value {
        Value::Num(num) => {
            native.tag = NATIVE_VALUE_NUM;
            native.num = *num;
        },
        Value::Str(text) => return Ok(native_string(NATIVE_VALUE_STRING, text, arena)),
        Value::Bool(boolean) => {
            native.tag = NATIVE_VALUE_BOOL;
            native.boolean = *boolean;
        },
        Value::Vec2(vec) => {
            native.tag = NATIVE_VALUE_VEC2;
            native.vec = [vec[0], vec[1], 0.0, 0.0];
        },
        Value::Vec3(vec) => {
            native.tag = NATIVE_VALUE_VEC3;
            native.vec = [vec[0], vec[1], vec[2], 0.0];
        },
        Value::Vec4(vec) => {
            native.tag = NATIVE_VALUE_VEC4;
            native.vec = *vec;
        },
        Value::Color(color) => {
            native.tag = NATIVE_VALUE_COLOR;
            native.vec = *color;
        },
        Value::List(items) => {
            let values = items
                .iter()
                .map(|item| value_to_native(item, arena))
                .collect::<Result<Vec<_>, _>>()?;
            let (list, list_len) = arena.list(values);
            native.tag = NATIVE_VALUE_LIST;
            native.list = list;
            native.list_len = list_len;
        },
        other => {
            return Err(EvalError::TypeMismatch(format!(
                "native plugin functions do not support {:?}",
                other
            )));
        },
    }
    Ok(native)
}

pub(crate) fn property_value_to_native(
    value: &PropertyValue,
    arena: &mut NativeValueArena,
) -> NativeValue {
    match value {
        PropertyValue::F32(value) => NativeValue {
            tag: NATIVE_VALUE_NUM,
            num: *value as f64,
            ..NativeValue::default()
        },
        PropertyValue::Bool(value) => NativeValue {
            tag: NATIVE_VALUE_BOOL,
            boolean: *value,
            ..NativeValue::default()
        },
        PropertyValue::U32(value) => NativeValue {
            tag: NATIVE_VALUE_U32,
            num: *value as f64,
            ..NativeValue::default()
        },
        PropertyValue::Vec2(value) => NativeValue {
            tag: NATIVE_VALUE_VEC2,
            vec: [value[0] as f64, value[1] as f64, 0.0, 0.0],
            ..NativeValue::default()
        },
        PropertyValue::Vec4(value) => NativeValue {
            tag: NATIVE_VALUE_VEC4,
            vec: [
                value[0] as f64,
                value[1] as f64,
                value[2] as f64,
                value[3] as f64,
            ],
            ..NativeValue::default()
        },
        PropertyValue::Color(value) => NativeValue {
            tag: NATIVE_VALUE_COLOR,
            vec: [
                value[0] as f64,
                value[1] as f64,
                value[2] as f64,
                value[3] as f64,
            ],
            ..NativeValue::default()
        },
        PropertyValue::String(value) => native_string(NATIVE_VALUE_STRING, value, arena),
        PropertyValue::PointList(points) => {
            let values = points
                .iter()
                .map(|point| NativeValue {
                    tag: NATIVE_VALUE_VEC2,
                    vec: [point[0] as f64, point[1] as f64, 0.0, 0.0],
                    ..NativeValue::default()
                })
                .collect();
            let (list, list_len) = arena.list(values);
            NativeValue {
                tag: NATIVE_VALUE_POINT_LIST,
                list,
                list_len,
                ..NativeValue::default()
            }
        },
        PropertyValue::F32List(values) => {
            let natives = values
                .iter()
                .map(|value| NativeValue {
                    tag: NATIVE_VALUE_NUM,
                    num: f64::from(*value),
                    ..NativeValue::default()
                })
                .collect();
            let (list, list_len) = arena.list(natives);
            NativeValue {
                tag: NATIVE_VALUE_LIST,
                list,
                list_len,
                ..NativeValue::default()
            }
        },
        PropertyValue::CommandList(value) => native_string(NATIVE_VALUE_COMMAND_LIST, value, arena),
        PropertyValue::StringList(values) => {
            let natives = values
                .iter()
                .map(|value| native_string(NATIVE_VALUE_STRING, value, arena))
                .collect();
            let (list, list_len) = arena.list(natives);
            NativeValue {
                tag: NATIVE_VALUE_STRING_LIST,
                list,
                list_len,
                ..NativeValue::default()
            }
        },
        PropertyValue::Transform(value) => NativeValue {
            tag: NATIVE_VALUE_TRANSFORM,
            transform: [
                value[0] as f64,
                value[1] as f64,
                value[2] as f64,
                value[3] as f64,
                value[4] as f64,
                value[5] as f64,
            ],
            ..NativeValue::default()
        },
        PropertyValue::Gradient(_) => {
            // No native brush tag exists yet; a plugin that needs to read a
            // ramp requires an ABI addition (see docs/handoff_motion_vocab.md
            // item 1). Logged rather than silently dropped.
            tracing::debug!(
                "property_value_to_native: gradient paints are not exposed over the plugin ABI yet"
            );
            NativeValue::default()
        },
        PropertyValue::Enum(value) => native_string(NATIVE_VALUE_ENUM, value, arena),
        PropertyValue::Variant { name, value } => {
            let (variant, variant_len) = arena.string(name);
            let payload = property_value_to_native(value, arena);
            let payload = arena.value(payload);
            NativeValue {
                tag: NATIVE_VALUE_VARIANT,
                string: variant,
                string_len: variant_len,
                payload,
                ..NativeValue::default()
            }
        },
    }
}

pub(crate) fn read_native_string(native: &NativeValue) -> Result<String, EvalError> {
    if native.string.is_null() {
        return Ok(String::new());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(native.string.cast::<u8>(), native.string_len) };
    String::from_utf8(bytes.to_vec()).map_err(|err| {
        EvalError::TypeMismatch(format!("native plugin returned invalid UTF-8 string: {err}"))
    })
}

pub(crate) fn read_native_list(native: &NativeValue) -> Result<Vec<Value>, EvalError> {
    if native.list.is_null() {
        return Ok(Vec::new());
    }
    let values = unsafe { std::slice::from_raw_parts(native.list, native.list_len) };
    values.iter().map(|value| native_to_value(*value)).collect()
}

pub(crate) fn native_to_value(native: NativeValue) -> Result<Value, EvalError> {
    match native.tag {
        NATIVE_VALUE_NUM => Ok(Value::Num(native.num)),
        NATIVE_VALUE_BOOL => Ok(Value::Bool(native.boolean)),
        NATIVE_VALUE_U32 => Ok(Value::Num(native.num)),
        NATIVE_VALUE_VEC2 => Ok(Value::Vec2([native.vec[0], native.vec[1]])),
        NATIVE_VALUE_VEC3 => Ok(Value::Vec3([native.vec[0], native.vec[1], native.vec[2]])),
        NATIVE_VALUE_VEC4 => Ok(Value::Vec4(native.vec)),
        NATIVE_VALUE_COLOR => Ok(Value::Color(native.vec)),
        NATIVE_VALUE_STRING | NATIVE_VALUE_COMMAND_LIST | NATIVE_VALUE_ENUM => {
            Ok(Value::Str(read_native_string(&native)?))
        },
        NATIVE_VALUE_LIST | NATIVE_VALUE_POINT_LIST | NATIVE_VALUE_STRING_LIST => {
            Ok(Value::List(read_native_list(&native)?.into()))
        },
        NATIVE_VALUE_TRANSFORM => Ok(Value::List(
            native
                .transform
                .iter()
                .map(|value| Value::Num(*value))
                .collect::<Vec<_>>()
                .into(),
        )),
        NATIVE_VALUE_VARIANT => Err(EvalError::TypeMismatch(
            "native plugin returned a variant value to an expression function".to_string(),
        )),
        tag => Err(EvalError::TypeMismatch(format!(
            "native plugin returned unknown value tag {tag}"
        ))),
    }
}

pub(crate) fn native_to_property_value(
    native: NativeValue,
    kind: animatix_syntax::schema::PropertyValueKind,
) -> Option<PropertyValue> {
    use animatix_syntax::schema::PropertyValueKind;
    match kind {
        PropertyValueKind::F32 => {
            (native.tag == NATIVE_VALUE_NUM).then_some(PropertyValue::F32(native.num as f32))
        },
        PropertyValueKind::U32 => (native.tag == NATIVE_VALUE_NUM
            || native.tag == NATIVE_VALUE_U32)
            .then_some(PropertyValue::U32(native.num.max(0.0) as u32)),
        PropertyValueKind::Bool => {
            (native.tag == NATIVE_VALUE_BOOL).then_some(PropertyValue::Bool(native.boolean))
        },
        PropertyValueKind::Vec2 => (native.tag == NATIVE_VALUE_VEC2)
            .then_some(PropertyValue::Vec2([native.vec[0] as f32, native.vec[1] as f32])),
        PropertyValueKind::Vec4 => (native.tag == NATIVE_VALUE_VEC4
            || native.tag == NATIVE_VALUE_COLOR)
            .then_some(PropertyValue::Vec4([
                native.vec[0] as f32,
                native.vec[1] as f32,
                native.vec[2] as f32,
                native.vec[3] as f32,
            ])),
        PropertyValueKind::String => read_native_string(&native).ok().map(PropertyValue::String),
        PropertyValueKind::PointList => {
            if native.tag != NATIVE_VALUE_POINT_LIST || native.list.is_null() {
                return None;
            }
            let values = unsafe { std::slice::from_raw_parts(native.list, native.list_len) };
            let points = values
                .iter()
                .filter(|value| value.tag == NATIVE_VALUE_VEC2)
                .map(|value| [value.vec[0] as f32, value.vec[1] as f32])
                .collect();
            Some(PropertyValue::PointList(points))
        },
        PropertyValueKind::Generic => match native.tag {
            NATIVE_VALUE_NUM => Some(PropertyValue::F32(native.num as f32)),
            NATIVE_VALUE_BOOL => Some(PropertyValue::Bool(native.boolean)),
            NATIVE_VALUE_VEC2 => {
                Some(PropertyValue::Vec2([native.vec[0] as f32, native.vec[1] as f32]))
            },
            NATIVE_VALUE_VEC4 => Some(PropertyValue::Vec4([
                native.vec[0] as f32,
                native.vec[1] as f32,
                native.vec[2] as f32,
                native.vec[3] as f32,
            ])),
            NATIVE_VALUE_COLOR => Some(PropertyValue::Color([
                native.vec[0] as f32,
                native.vec[1] as f32,
                native.vec[2] as f32,
                native.vec[3] as f32,
            ])),
            NATIVE_VALUE_STRING => read_native_string(&native).ok().map(PropertyValue::String),
            _ => None,
        },
    }
}

/// Decode an easing code a plugin sent.
pub(crate) fn native_easing(code: u32) -> Option<crate::easing::Easing> {
    use crate::easing::Easing;
    match code {
        NATIVE_EASING_LINEAR => Some(Easing::Linear),
        NATIVE_EASING_IN => Some(Easing::EaseIn),
        NATIVE_EASING_OUT => Some(Easing::EaseOut),
        NATIVE_EASING_IN_OUT => Some(Easing::EaseInOut),
        NATIVE_EASING_BOUNCE_IN => Some(Easing::Bounce),
        NATIVE_EASING_ELASTIC => Some(Easing::Elastic),
        NATIVE_EASING_BACK => Some(Easing::Back),
        NATIVE_EASING_EXPO => Some(Easing::Expo),
        NATIVE_EASING_EXPO_OUT => Some(Easing::ExpoOut),
        NATIVE_EASING_EXPO_IN_OUT => Some(Easing::ExpoInOut),
        _ => None,
    }
}

/// Encode an easing curve for a plugin.
pub(crate) fn easing_code(easing: crate::easing::Easing) -> u32 {
    use crate::easing::Easing;
    match easing {
        Easing::Linear => NATIVE_EASING_LINEAR,
        Easing::EaseIn => NATIVE_EASING_IN,
        Easing::EaseOut => NATIVE_EASING_OUT,
        Easing::EaseInOut => NATIVE_EASING_IN_OUT,
        Easing::Bounce => NATIVE_EASING_BOUNCE_IN,
        Easing::Elastic => NATIVE_EASING_ELASTIC,
        Easing::Back => NATIVE_EASING_BACK,
        Easing::Expo => NATIVE_EASING_EXPO,
        Easing::ExpoOut => NATIVE_EASING_EXPO_OUT,
        Easing::ExpoInOut => NATIVE_EASING_EXPO_IN_OUT,
        Easing::Spring { .. } | Easing::SpringV0 { .. } | Easing::CubicBezier(_) => {
            NATIVE_EASING_UNSUPPORTED
        },
    }
}
