use mitex::convert_math;
use typst::World;
use typst::foundations::{Bytes, Datetime};
use typst::layout::{Frame, FrameItem, Transform};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt};

#[derive(Clone)]
pub struct ExtractedGlyph {
    pub font: Font,
    pub glyph_id: u16,
    pub transform: Transform,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
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

pub fn extract_glyphs(frame: &Frame) -> Vec<ExtractedGlyph> {
    let mut glyphs = Vec::new();
    walk_frame_for_glyphs(frame, Transform::identity(), &mut glyphs);
    glyphs
}

fn walk_frame_for_glyphs(
    frame: &Frame,
    current_transform: Transform,
    glyphs: &mut Vec<ExtractedGlyph>,
) {
    for (pos, item) in frame.items() {
        let transform = current_transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                let group_transform = transform.pre_concat(group.transform);
                walk_frame_for_glyphs(&group.frame, group_transform, glyphs);
            }
            FrameItem::Text(text) => {
                let size = text.size.to_pt() as f32;
                let mut x_curr = 0.0;
                for glyph in &text.glyphs {
                    let offset = glyph.x_offset.at(text.size).to_pt() as f32;
                    let advance = glyph.x_advance.at(text.size).to_pt() as f32;

                    let x = transform.tx.to_pt() as f32 + x_curr + offset;
                    let y = transform.ty.to_pt() as f32;
                    let scale = size;

                    glyphs.push(ExtractedGlyph {
                        font: text.font.clone(),
                        glyph_id: glyph.id,
                        transform,
                        x,
                        y,
                        scale,
                    });

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
