//! Shared scene builders and fixtures for criterion benchmarks.
#![allow(dead_code)] // each merged bench target includes this module and uses a subset

use animatix::timeline::Timeline;

pub fn build_scene(actor_count: usize) -> Timeline {
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..actor_count {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 30) * 60,
            100 + (i / 30) * 60
        ));
    }
    crate::common::parse_timeline(&source)
}

pub fn build_static_scene(actor_count: usize) -> Timeline {
    // Static scene: no keyframes, no modifiers.
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..actor_count {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 20) * 90,
            100 + (i / 20) * 90
        ));
    }
    crate::common::parse_timeline(&source)
}

pub fn build_many_actors_scene() -> Timeline {
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..50 {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + i * 30,
            100 + (i % 10) * 50
        ));
    }
    crate::common::parse_timeline(&source)
}

pub fn build_visible_scene(actor_count: usize) -> Timeline {
    // All actors visible on screen
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..actor_count {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 20) * 90,
            100 + (i / 20) * 90
        ));
    }
    crate::common::parse_timeline(&source)
}
