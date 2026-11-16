//! Frozen v1 CPU geometry workload. Changes to these inputs require a new ID.
use std::fmt::Write;

use animatix::timeline::{SceneDimensions, Timeline};

pub const GROUP: &str = "geometry_rect_count_v1";
pub const COUNTS: [usize; 3] = [16, 64, 256];
pub const DIMENSIONS: SceneDimensions = SceneDimensions {
    width: 1024,
    height: 1024,
};
pub const STATIC_TIME: f64 = 0.5;
pub const SAMPLE_COUNT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    StaticRepeatedTime,
    AnimatedVaryingTime,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Self::StaticRepeatedTime => "static_repeated_time",
            Self::AnimatedVaryingTime => "animated_varying_time",
        }
    }
}

/// Prefixes of one fixed 16-column grid: count is the only scaling parameter.
pub fn source(count: usize, mode: Mode) -> String {
    assert!(COUNTS.contains(&count), "unsupported frozen geometry count: {count}");
    let mut source = String::from("config { resolution: (1024, 1024) }\n#0s\n");
    for i in 0..count {
        let x = 32 + (i % 16) * 56;
        let y = 32 + (i / 16) * 56;
        writeln!(source, "r{i}: Rect, at: ({x}, {y}), size: (24, 24), color: (1, 0, 0, 1), opacity: 1, stroke_width: 0").unwrap();
    }
    if mode == Mode::AnimatedVaryingTime {
        for i in 0..count {
            let x = 40 + (i % 16) * 56;
            let y = 32 + (i / 16) * 56;
            writeln!(source, "r{i}.at = ({x}, {y}) [1s, ease: linear]").unwrap();
        }
    }
    source
}

/// Called outside the timed closure. Parse/build failures and count drift fail fast.
pub fn build(count: usize, mode: Mode) -> Timeline {
    let (ast, errors) = animatix_syntax::parser::parse_source(&source(count, mode));
    assert!(errors.is_empty(), "geometry fixture parse: {errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.expect("geometry AST"), &Default::default());
    assert!(
        report.diagnostics.is_empty(),
        "geometry fixture build: {:?}",
        report.diagnostics
    );
    assert_eq!(report.output.actor_labels().count(), count, "geometry actor count");
    report.output
}

/// Exact binary fractions: consecutive samples also differ after millisecond
/// quantization, including wrap. This does not disable subtree/geometry caches.
#[derive(Default)]
pub struct SampleClock {
    index: usize,
}

impl SampleClock {
    pub fn next_time(&mut self) -> f64 {
        let time = self.index as f64 / SAMPLE_COUNT as f64;
        self.index = (self.index + 1) % SAMPLE_COUNT;
        time
    }
}
