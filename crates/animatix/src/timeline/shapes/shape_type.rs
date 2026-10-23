use super::*;

/// Discriminant for the kind of geometric shape an actor represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ShapeType {
    /// Rectangle.
    #[default]
    Rect = 0,
    /// Ellipse (or arc when `arc_angles` are non-zero).
    Ellipse = 1,
    /// Line, optionally with an arrow tip.
    Line = 2,
    /// Polygon, regular or custom.
    Polygon = 3,
    /// Free-form path defined by Bézier commands.
    Path = 4,
    /// Graph / coordinate plane.
    Graph = 5,
    /// Plot curve.
    Plot = 6,
    /// Arrow with a dedicated arrowhead.
    Arrow = 7,
}

impl ShapeType {
    /// Return the static string name of this variant.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Rect => "Rect",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
            Self::Polygon => "Polygon",
            Self::Path => "Path",
            Self::Graph => "Graph",
            Self::Plot => "Plot",
            Self::Arrow => "Arrow",
        }
    }
}

impl std::str::FromStr for ShapeType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Rect" => Ok(Self::Rect),
            "Ellipse" => Ok(Self::Ellipse),
            "Line" => Ok(Self::Line),
            "Polygon" => Ok(Self::Polygon),
            "Path" => Ok(Self::Path),
            "Graph" => Ok(Self::Graph),
            "PlotCurve" => Ok(Self::Plot),
            "Arrow" => Ok(Self::Arrow),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for ShapeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<u32> for ShapeType {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Rect,
            1 => Self::Ellipse,
            2 => Self::Line,
            3 => Self::Polygon,
            4 => Self::Path,
            5 => Self::Graph,
            6 => Self::Plot,
            7 => Self::Arrow,
            _ => Self::Rect,
        }
    }
}

impl Interpolate for ShapeType {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}
