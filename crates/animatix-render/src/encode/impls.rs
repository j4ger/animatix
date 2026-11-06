//! Trait implementations for the shared export configuration types.

use super::{
    ExportOptions, ExportPreset, ExportSettings, H264Preset, MaxRenderThreads, VideoCodec,
};

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            max_render_threads: MaxRenderThreads::Auto,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Medium,
        }
    }
}

impl<'a> ExportOptions<'a> {
    /// Options for a settings-only export (no progress or cancellation).
    pub fn with_settings(settings: ExportSettings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }
}

impl ExportPreset {
    /// All named presets available to CLI and GUI.
    pub const ALL: &'static [ExportPreset] = &[
        ExportPreset {
            name: "720p30",
            width: 1280,
            height: 720,
            fps: 30,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Medium,
        },
        ExportPreset {
            name: "1080p30",
            width: 1920,
            height: 1080,
            fps: 30,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Medium,
        },
        ExportPreset {
            name: "1080p60",
            width: 1920,
            height: 1080,
            fps: 60,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Fast,
        },
        ExportPreset {
            name: "4k30",
            width: 3840,
            height: 2160,
            fps: 30,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Slow,
        },
    ];

    /// Look up a named export preset.
    pub fn by_name(name: &str) -> Option<&'static ExportPreset> {
        Self::ALL.iter().find(|preset| preset.name == name)
    }
}

impl std::fmt::Display for MaxRenderThreads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Fixed(n) => write!(f, "{n}"),
        }
    }
}

impl std::str::FromStr for MaxRenderThreads {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            Ok(Self::Auto)
        } else {
            let n = s.parse::<usize>().map_err(|e| format!("Invalid thread count: {e}"))?;
            Ok(Self::Fixed(n))
        }
    }
}

impl VideoCodec {
    /// Every encoder choice, in the order the GUI lists them.
    pub const ALL: &[VideoCodec] = &[
        Self::Auto,
        Self::Libx264,
        Self::H264Nvenc,
        Self::H264Vaapi,
        Self::Vp9,
    ];
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Libx264 => write!(f, "libx264"),
            Self::H264Nvenc => write!(f, "h264_nvenc"),
            Self::H264Vaapi => write!(f, "h264_vaapi"),
            Self::Vp9 => write!(f, "libvpx-vp9"),
        }
    }
}

impl std::str::FromStr for VideoCodec {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "libx264" => Ok(Self::Libx264),
            "h264_nvenc" | "nvenc" => Ok(Self::H264Nvenc),
            "h264_vaapi" | "vaapi" => Ok(Self::H264Vaapi),
            "vp9" | "libvpx-vp9" => Ok(Self::Vp9),
            _ => Err(format!(
                "Unknown codec: {s}. Expected: auto, libx264, h264_nvenc, h264_vaapi, vp9"
            )),
        }
    }
}

impl H264Preset {
    /// Every preset, fastest first — the order the GUI and CLI list them in.
    pub const ALL: &[H264Preset] = &[
        Self::Ultrafast,
        Self::Superfast,
        Self::Veryfast,
        Self::Faster,
        Self::Fast,
        Self::Medium,
        Self::Slow,
        Self::Slower,
        Self::Veryslow,
    ];

    /// Returns the ffmpeg preset name.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

impl std::fmt::Display for H264Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for H264Preset {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ultrafast" => Ok(Self::Ultrafast),
            "superfast" => Ok(Self::Superfast),
            "veryfast" => Ok(Self::Veryfast),
            "faster" => Ok(Self::Faster),
            "fast" => Ok(Self::Fast),
            "medium" => Ok(Self::Medium),
            "slow" => Ok(Self::Slow),
            "slower" => Ok(Self::Slower),
            "veryslow" => Ok(Self::Veryslow),
            _ => Err(format!(
                "Unknown preset: {s}. Expected: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow"
            )),
        }
    }
}
