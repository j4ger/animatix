use std::ffi::CString;
use std::path::Path;

use animatix::timeline::video::VideoMetadata;
use rsmpeg::avcodec::{AVCodec, AVCodecParameters};
use rsmpeg::avformat::{AVFormatContextInput, AVInputFormat};
use rsmpeg::avutil::AVDictionary;
use rsmpeg::ffi::{self, AVRational};

#[derive(Clone, Copy)]
pub(super) struct Media {
    pub metadata: VideoMetadata,
    pub stream_index: usize,
    pub time_base: AVRational,
    pub origin_s: f64,
    pub video_start_s: f64,
    pub scaled_size: [u32; 2],
    pub orientation: Orientation,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Orientation {
    pub transpose: bool,
    pub flip_x: bool,
    pub flip_y: bool,
}

pub(super) fn open_input(path: &Path) -> Result<AVFormatContextInput, String> {
    let url =
        CString::new(path.as_os_str().as_encoded_bytes()).map_err(|error| error.to_string())?;
    let format = AVInputFormat::find(c"mov").ok_or("FFmpeg has no MP4/MOV demuxer")?;
    let mut options = Some(AVDictionary::new(c"protocol_whitelist", c"file", 0));
    AVFormatContextInput::builder()
        .url(&url)
        .format(&format)
        .options(&mut options)
        .open()
        .map_err(|error| format!("Could not open '{}': {error}", path.display()))
}

pub(super) fn read(path: &Path) -> Result<Media, String> {
    let input = open_input(path)?;
    let streams = input.streams();
    let (stream_index, video) = streams
        .iter()
        .enumerate()
        .find(|(_, stream)| {
            stream.codecpar().codec_type == ffi::AVMEDIA_TYPE_VIDEO
                && stream.disposition & ffi::AV_DISPOSITION_ATTACHED_PIC as i32 == 0
        })
        .ok_or("Video source has no playable video stream")?;
    let codec = video.codecpar();
    if AVCodec::find_decoder(codec.codec_id).is_none() {
        return Err("FFmpeg has no decoder for this video stream".into());
    }
    let origin_s = media_seconds(input.start_time).unwrap_or(0.0);
    let video_start_s =
        seconds(video.start_time, video.time_base).map_or(0.0, |time| time - origin_s);
    let duration_s = seconds(video.duration, video.time_base)
        .filter(|duration| *duration > 0.0)
        .map(|duration| duration + video_start_s)
        .or_else(|| media_seconds(input.duration))
        .filter(|duration| *duration > 0.0)
        .ok_or("Video source has no positive duration")?;
    let audio_start_s = streams
        .iter()
        .find(|stream| stream.codecpar().codec_type == ffi::AVMEDIA_TYPE_AUDIO)
        .map(|stream| {
            seconds(stream.start_time, stream.time_base).map_or(0.0, |time| time - origin_s)
        });
    let sar = video.sample_aspect_ratio;
    let sar = if sar.num > 0 && sar.den > 0 {
        sar
    } else {
        codec.sample_aspect_ratio
    };
    let scaled_size = scaled_size(codec.width, codec.height, sar)?;
    let orientation = orientation(&codec)?;
    let [width, height] = scaled_size;
    let display_size = if orientation.transpose {
        [height, width]
    } else {
        [width, height]
    };
    Ok(Media {
        metadata: VideoMetadata {
            display_size,
            duration_s,
            audio_start_s,
        },
        stream_index,
        time_base: video.time_base,
        origin_s,
        video_start_s,
        scaled_size,
        orientation,
    })
}

fn media_seconds(value: i64) -> Option<f64> {
    (value != ffi::AV_NOPTS_VALUE).then(|| value as f64 / f64::from(ffi::AV_TIME_BASE))
}

pub(super) fn seconds(value: i64, base: AVRational) -> Option<f64> {
    (value != ffi::AV_NOPTS_VALUE && base.num > 0 && base.den > 0)
        .then(|| value as f64 * f64::from(base.num) / f64::from(base.den))
}

fn scaled_size(width: i32, height: i32, sar: AVRational) -> Result<[u32; 2], String> {
    let ratio = if sar.num > 0 && sar.den > 0 {
        f64::from(sar.num) / f64::from(sar.den)
    } else {
        1.0
    };
    let size = [
        (f64::from(width) * ratio).round() as u32,
        height.max(0) as u32,
    ];
    let [width, height] = size;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
    {
        return Err(
            "Video display size exceeds the decoder limits (8192 px per side, 16 Mi pixels)".into(),
        );
    }
    Ok(size)
}

fn orientation(codec: &AVCodecParameters) -> Result<Orientation, String> {
    // FFmpeg owns the stream's side data for the lifetime of the codec parameters.
    let Some(matrix) = (unsafe {
        ffi::av_packet_side_data_get(
            codec.coded_side_data,
            codec.nb_coded_side_data,
            ffi::AV_PKT_DATA_DISPLAYMATRIX,
        )
        .as_ref()
    }) else {
        return Ok(Orientation::default());
    };
    if matrix.data.is_null() || matrix.size < std::mem::size_of::<[i32; 9]>() {
        return Err("Invalid video display matrix".into());
    }
    // FFmpeg stores the matrix as nine native-endian i32 values.
    let matrix = unsafe { matrix.data.cast::<[i32; 9]>().read_unaligned() };
    let rotation = unsafe { ffi::av_display_rotation_get(matrix.as_ptr()) };
    let turns = (rotation / 90.0).round();
    if !rotation.is_finite() || (rotation - turns * 90.0).abs() > 0.01 {
        return Err("Video display rotation must be a multiple of 90 degrees".into());
    }
    let transpose = (turns as i32).rem_euclid(2) == 1;
    let (x, y) = if transpose {
        (matrix[3], matrix[1])
    } else {
        (matrix[0], matrix[4])
    };
    Ok(Orientation {
        transpose,
        flip_x: x < 0,
        flip_y: y < 0,
    })
}
