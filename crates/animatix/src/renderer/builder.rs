use super::types::SdfInstance;
use crate::ast::{Expr, Stmt};

pub fn parse_color(expr: &Expr) -> [f32; 4] {
    if let Expr::Ident(name) = expr {
        match name.as_str() {
            "red" => [1.0, 0.0, 0.0, 1.0],
            "green" => [0.0, 1.0, 0.0, 1.0],
            "blue" => [0.0, 0.0, 1.0, 1.0],
            "black" => [0.0, 0.0, 0.0, 1.0],
            "white" => [1.0, 1.0, 1.0, 1.0],
            _ => [0.8, 0.8, 0.8, 1.0],
        }
    } else {
        [0.8, 0.8, 0.8, 1.0]
    }
}

pub fn build_instances(ast: &[Stmt]) -> Vec<SdfInstance> {
    let mut instances = Vec::new();

    for stmt in ast {
        if let Stmt::Keyframe { body, .. } = stmt {
            for sub_stmt in body {
                if let Stmt::ActorDecl { ty, props, .. } = sub_stmt {
                    let mut pos = [0.0, 0.0];
                    let mut size = [50.0, 50.0];
                    let mut color = [1.0, 1.0, 1.0, 1.0];
                    let is_circle = if ty == "Circle" { 1 } else { 0 };

                    for prop in props {
                        match prop.name.as_str() {
                            "at" => {
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        if let Expr::Num(x) = arr[0] {
                                            pos[0] = x as f32;
                                        }
                                        if let Expr::Num(y) = arr[1] {
                                            pos[1] = y as f32;
                                        }
                                    }
                                }
                            }
                            "radius" => {
                                if let Expr::Num(r) = prop.value {
                                    size = [r as f32, r as f32];
                                }
                            }
                            "size" => {
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        if let Expr::Num(w) = arr[0] {
                                            size[0] = w as f32 / 2.0;
                                        }
                                        if let Expr::Num(h) = arr[1] {
                                            size[1] = h as f32 / 2.0;
                                        }
                                    }
                                }
                            }
                            "color" => {
                                color = parse_color(&prop.value);
                            }
                            _ => {}
                        }
                    }

                    instances.push(SdfInstance {
                        position: pos,
                        size,
                        uv_rect: [0.0; 4],
                        shape_params: [0.0; 4],
                        fill_color: color,
                        stroke_color: [0.0; 4],
                        stroke_width: 0.0,
                        glow_radius: 0.0,
                        opacity: 1.0,
                        shape_type: is_circle,
                        target_position: pos,
                        target_size: size,
                        target_shape_params: [0.0; 4],
                        target_shape_type: is_circle,
                        shape_blend: 0.0,
                        _padding1: [0.0; 2],
                        morph_params: [0.0; 4],
                    });
                }
            }
        }
    }

    instances
}
