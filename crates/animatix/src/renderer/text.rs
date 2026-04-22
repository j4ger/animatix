pub use super::types::TextPath;
use kurbo::{Affine, BezPath, Point, Shape};
use mitex::convert_math;
use typst::foundations::{Bytes, Datetime};
use typst::layout::{Frame, FrameItem, Transform};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::World;
use typst::{Library, LibraryExt};

struct PathBuilder(BezPath);

impl ttf_parser::OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(Point::new(x as f64, y as f64));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(Point::new(x as f64, y as f64));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.quad_to(
            Point::new(x1 as f64, y1 as f64),
            Point::new(x as f64, y as f64),
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.curve_to(
            Point::new(x1 as f64, y1 as f64),
            Point::new(x2 as f64, y2 as f64),
            Point::new(x as f64, y as f64),
        );
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}

#[derive(Clone)]
pub struct ExtractedShape {
    pub curve: typst::visualize::Curve,
    pub transform: Transform,
}

pub struct TypstWorld {
    source: Source,
    font: Font,
    math_font: Font,
    book: LazyHash<FontBook>,
    library: LazyHash<Library>,
}

impl TypstWorld {
    pub fn new(source: Source) -> Self {
        let font_data = include_bytes!("../../assets/mock_font.ttf");
        let font = Font::new(Bytes::new(font_data), 0)
            .expect("Failed to load mock font. Replace with real font later.");

        let math_font_data = include_bytes!("../../assets/fonts/FiraMath-Regular.otf");
        let math_font =
            Font::new(Bytes::new(math_font_data), 0).expect("Failed to load math font.");

        let mut book = FontBook::new();
        book.push(font.info().clone());
        book.push(math_font.info().clone());
        let library = typst::Library::builder().build();

        Self {
            source,
            font,
            math_font,
            book: LazyHash::new(book),
            library: LazyHash::new(library),
        }
    }
}

impl World for TypstWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> typst::diag::FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(typst::diag::FileError::NotFound(
                id.vpath().as_rootless_path().into(),
            ))
        }
    }

    fn file(&self, id: FileId) -> typst::diag::FileResult<Bytes> {
        Err(typst::diag::FileError::NotFound(
            id.vpath().as_rootless_path().into(),
        ))
    }

    fn font(&self, index: usize) -> Option<Font> {
        if index == 0 {
            Some(self.font.clone())
        } else if index == 1 {
            Some(self.math_font.clone())
        } else {
            None
        }
    }

    fn today(&self, _offset: Option<i64>) -> Option<Datetime> {
        None
    }
}

pub fn compile_math(latex: &str, font_size: f32, color: typst::visualize::Color) -> Frame {
    let typst_markup = convert_math(latex, None).unwrap();
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: (\"Open Sans\", \"Fira Math\")); #show math.equation: set text(font: \"Fira Math\"); $ {} $",
        font_size,
        color.to_hex(),
        typst_markup
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::new(source);
    let document: typst::layout::PagedDocument = typst::compile(&world).output.unwrap();

    document.pages[0].frame.clone()
}

pub fn compile_text(text: &str, font_size: f32, color: typst::visualize::Color) -> Frame {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: \"Open Sans\")\n{}",
        font_size,
        color.to_hex(),
        escaped
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::new(source);
    let document: typst::layout::PagedDocument = typst::compile(&world).output.unwrap();

    document.pages[0].frame.clone()
}

pub fn compile_code(code: &str, font_size: f32, color: typst::visualize::Color) -> Frame {
    let escaped = code
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let markup = format!(
        "#set text(size: {}pt, fill: rgb(\"{}\"), font: \"Open Sans\")\n{}",
        font_size,
        color.to_hex(),
        escaped
    );

    let source = Source::new(FileId::new(None, VirtualPath::new("main.typ")), markup);
    let world = TypstWorld::new(source);
    let document: typst::layout::PagedDocument = typst::compile(&world).output.unwrap();

    document.pages[0].frame.clone()
}

pub fn extract_glyphs(frame: &Frame) -> Vec<TextPath> {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);
    center_text_paths(&mut glyphs);
    glyphs
}

/// Centers text paths around the origin so that layout positioning works correctly.
/// The layout system positions children by their center point, so text needs to be
/// centered around (0, 0) for layout alignment to work properly.
pub fn center_text_paths(paths: &mut [TextPath]) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for path in paths.iter() {
        let bounds = path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        let center_x = (min_x + max_x) / 2.0;
        let center_y = (min_y + max_y) / 2.0;
        let offset = Affine::translate((-center_x, -center_y));

        for path in paths.iter_mut() {
            path.path.apply_affine(offset);
        }
    }
}

pub fn measure_text_paths(paths: &[TextPath]) -> [f32; 2] {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for text_path in paths {
        let bounds = text_path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }

    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        [
            ((max_x - min_x) as f32) / 2.0,
            ((max_y - min_y) as f32) / 2.0,
        ]
    } else {
        [0.0, 0.0]
    }
}

fn walk_frame_for_glyphs(frame: &Frame, current_transform: Transform, glyphs: &mut Vec<TextPath>) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs(&group.frame, group_transform, glyphs);
            }
            FrameItem::Text(text) => {
                let size = text.size.to_pt() as f32;
                let units_per_em = text.font.units_per_em() as f32;
                let font_scale = size / units_per_em;
                let face = text.font.ttf();

                let mut x_curr = 0.0;
                for glyph in &text.glyphs {
                    let offset_x = glyph.x_offset.at(text.size).to_pt() as f32;
                    let offset_y = glyph.y_offset.at(text.size).to_pt() as f32;
                    let advance = glyph.x_advance.at(text.size).to_pt() as f32;

                    let mut builder = PathBuilder(BezPath::new());
                    if let Some(_bounds) =
                        face.outline_glyph(ttf_parser::GlyphId(glyph.id), &mut builder)
                    {
                        let path = builder.0;

                        let scale_affine =
                            Affine::scale_non_uniform(font_scale as f64, -font_scale as f64);

                        let glyph_translate = Affine::translate(kurbo::Vec2::new(
                            (x_curr + offset_x) as f64,
                            offset_y as f64,
                        ));

                        let item_transform = Affine::new([
                            transform.sx.get() as f64,
                            transform.ky.get() as f64,
                            transform.kx.get() as f64,
                            transform.sy.get() as f64,
                            transform.tx.to_pt() as f64,
                            transform.ty.to_pt() as f64,
                        ]);

                        let final_affine = item_transform * glyph_translate * scale_affine;
                        let mut final_path = path;
                        final_path.apply_affine(final_affine);

                        glyphs.push(TextPath {
                            path: final_path,
                            color: text.fill.clone(),
                        });
                    }

                    x_curr += advance;
                }
            }
            _ => {}
        }
    }
}

pub fn extract_shapes(frame: &Frame) -> Vec<ExtractedShape> {
    let mut shapes = Vec::new();
    walk_frame_for_shapes(frame, Transform::identity(), &mut shapes);
    shapes
}

fn walk_frame_for_shapes(
    frame: &Frame,
    current_transform: Transform,
    shapes: &mut Vec<ExtractedShape>,
) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_shapes(&group.frame, group_transform, shapes);
            }
            FrameItem::Shape(shape, _span) => {
                if let typst::visualize::Geometry::Curve(curve) = &shape.geometry {
                    shapes.push(ExtractedShape {
                        curve: (*curve).clone(),
                        transform,
                    });
                }
            }
            _ => {}
        }
    }
}
