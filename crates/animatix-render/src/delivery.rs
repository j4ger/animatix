//! Starter project and native capability checks shared by the CLI and workbench.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const SCENE: &str = "main.amx";
const LAYOUT: (&str, &[u8]) =
    ("layout.amx", include_bytes!("../../../web/content/templates/layout.amx"));
const NARRATED: [(&str, &[u8]); 4] = [
    (SCENE, include_bytes!("delivery/starter.amx")),
    LAYOUT,
    ("narration.mp3", include_bytes!("delivery/narration.mp3")),
    ("narration.speech.json", include_bytes!("delivery/narration.speech.json")),
];
const MATH_EXPLAINER: [(&str, &[u8]); 2] = [
    (SCENE, include_bytes!("delivery/math-explainer.amx")),
    LAYOUT,
];
const METRIC_BRIEF: [(&str, &[u8]); 2] =
    [(SCENE, include_bytes!("delivery/metric-brief.amx")), LAYOUT];

/// A self-contained starter available in both native authoring tools.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectTemplate {
    /// A timed narrative with a bundled voice and captions.
    #[default]
    Narrated,
    /// A graph, math note, and takeaway.
    MathExplainer,
    /// A status summary with reusable metric cards.
    MetricBrief,
}

impl ProjectTemplate {
    pub const ALL: [Self; 3] = [Self::Narrated, Self::MathExplainer, Self::MetricBrief];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Narrated => "narrated",
            Self::MathExplainer => "math-explainer",
            Self::MetricBrief => "metric-brief",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Narrated => "Narrated Explainer",
            Self::MathExplainer => "Math Explainer",
            Self::MetricBrief => "Metric Brief",
        }
    }

    /// Create this starter in a new or empty directory and return its scene path.
    pub fn create(self, directory: &Path) -> io::Result<PathBuf> {
        write_project(
            directory,
            match self {
                Self::Narrated => &NARRATED,
                Self::MathExplainer => &MATH_EXPLAINER,
                Self::MetricBrief => &METRIC_BRIEF,
            },
        )
    }
}

impl std::str::FromStr for ProjectTemplate {
    type Err = io::Error;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        Self::ALL.into_iter().find(|template| template.id() == id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, format!("unknown template '{id}'"))
        })
    }
}

/// Create a self-contained narrated project in a new or empty directory.
pub fn create_project(directory: &Path) -> io::Result<PathBuf> {
    ProjectTemplate::Narrated.create(directory)
}

/// Create a math explainer with a sample graph and a reusable layout component.
pub fn create_math_explainer_project(directory: &Path) -> io::Result<PathBuf> {
    ProjectTemplate::MathExplainer.create(directory)
}

fn write_project(directory: &Path, files: &[(&str, &[u8])]) -> io::Result<PathBuf> {
    let created_dir = match fs::create_dir(directory) {
        Ok(()) => true,
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            if fs::read_dir(directory)?.next().transpose()?.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "choose an empty project directory",
                ));
            }
            false
        },
        Err(err) => return Err(err),
    };
    let mut created = Vec::with_capacity(files.len());
    let result = (|| {
        for (name, content) in files {
            let path = directory.join(name);
            let mut file = OpenOptions::new().write(true).create_new(true).open(&path)?;
            created.push(path);
            file.write_all(content)?;
        }
        Ok(directory.join(SCENE))
    })();
    if result.is_err() {
        for path in created {
            let _ = fs::remove_file(path);
        }
        if created_dir {
            let _ = fs::remove_dir(directory);
        }
    }
    result
}

fn doctor() -> [(&'static str, Result<(), String>); 4] {
    use animatix::renderer::text::{FontContext, compile_text_fast};

    let gpu = crate::offscreen::OffscreenRenderer::new().map(|_| ()).map_err(|err| {
        if cfg!(target_os = "linux") {
            format!("{err}. Install a compatible graphics driver, or Mesa lavapipe for headless rendering (Debian/Ubuntu: libvulkan1 and mesa-vulkan-drivers).")
        } else {
            format!("{err}. Install or update a compatible graphics driver and run in a graphics-capable session.")
        }
    });
    let fonts = compile_text_fast(
        "Animatix",
        "Open Sans",
        400.0,
        "normal",
        24.0,
        [1.0; 4],
        0.0,
        0.0,
        &FontContext::new(),
    )
    .map_err(|err| err.to_string())
    .and_then(|text| {
        if text.glyphs.is_empty() {
            Err("No glyphs were produced".into())
        } else {
            Ok(())
        }
    })
    .map_err(|err| format!("{err}. Reinstall the Animatix package or install Open Sans."));
    [
        ("GPU preview and rendering", gpu),
        ("Text shaping", fonts),
        ("AAC audio encoder", ffmpeg_encoder(c"aac")),
        ("H.264 video encoder", ffmpeg_encoder(c"libx264")),
    ]
}

#[cfg(feature = "video")]
fn ffmpeg_encoder(name: &std::ffi::CStr) -> Result<(), String> {
    if rsmpeg::avcodec::AVCodec::find_encoder_by_name(name).is_some() {
        Ok(())
    } else {
        Err(format!(
            "This FFmpeg was built without {}. Install a video-enabled Animatix package.",
            name.to_string_lossy()
        ))
    }
}

#[cfg(not(feature = "video"))]
fn ffmpeg_encoder(_: &std::ffi::CStr) -> Result<(), String> {
    Err("Video export is disabled in this build. Install a video-enabled Animatix package.".into())
}

/// Print the capability check for the CLI and for workbench startup diagnosis.
/// Returns whether all prerequisites are ready.
pub fn print_doctor() -> bool {
    let mut ready = true;
    for (name, result) in doctor() {
        match result {
            Ok(()) => println!("OK      {name}"),
            Err(err) => {
                println!("MISSING {name}: {err}");
                ready = false;
            },
        }
    }
    ready
}
