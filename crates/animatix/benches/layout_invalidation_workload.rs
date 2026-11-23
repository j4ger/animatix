//! Frozen layout invalidation workload v1; no internal cache-hit claim.
use std::fmt::Write;

use animatix::timeline::{SceneDimensions, Timeline};

pub const GROUP: &str = "layout_invalidation_v1";
pub const COUNTS: [usize; 2] = [16, 64];
pub const SAMPLE_COUNT: usize = 64;
pub const DIMENSIONS: SceneDimensions = SceneDimensions {
    width: 2048,
    height: 256,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Static,
    Color,
    Size,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Color => "color",
            Self::Size => "size",
        }
    }
}

pub fn source(count: usize, mode: Mode) -> String {
    assert!(COUNTS.contains(&count), "unsupported frozen Row child count: {count}");
    let mut source = String::from(
        "config { canvas_size: (2048, 256), dynamic_layout: true }\n#0s\n\
         row: Row, size: (1792, 128), at: (1024, 128), gap: 4, align: \"center\", opacity: 1 {\n",
    );
    for i in 0..count {
        writeln!(
            source,
            "  r{i}: Rect, size: (12, 24), color: (1, 0, 0, 1), opacity: 1, stroke_width: 0"
        )
        .unwrap();
    }
    source.push_str("}\n");
    for i in 0..count {
        match mode {
            Mode::Static => {},
            Mode::Color => {
                writeln!(source, "r{i}.color = (0, 0, 1, 1) [1s, ease: linear]").unwrap()
            },
            Mode::Size => writeln!(source, "r{i}.size = (20, 24) [1s, ease: linear]").unwrap(),
        }
    }
    source
}

/// Input generation/parse/build/count validation are outside measurement.
pub fn build(count: usize, mode: Mode) -> Timeline {
    let (ast, errors) = animatix_syntax::parser::parse_source(&source(count, mode));
    assert!(errors.is_empty(), "layout fixture parse: {errors:?}");
    let report =
        Timeline::build_with_diagnostics(&ast.expect("layout fixture AST"), &Default::default());
    assert!(report.diagnostics.is_empty(), "layout fixture build: {:?}", report.diagnostics);
    assert_eq!(report.output.actor_labels().count(), count + 1, "Rects plus one Row");
    report.output
}

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
