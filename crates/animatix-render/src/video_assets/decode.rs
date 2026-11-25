use std::path::Path;
use std::ptr::NonNull;

use animatix::timeline::image::SceneImage;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avformat::AVFormatContextInput;
use rsmpeg::avutil::{AVDictionary, AVFrame};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi;
use rsmpeg::swscale::SwsContext;

use super::probe::{Media, open_input, seconds};

pub(super) struct Decoder {
    input: AVFormatContextInput,
    codec: AVCodecContext,
    scaler: SwsContext,
    media: Media,
    sought: bool,
    previous: Option<Frame>,
    next: Option<Frame>,
    draining: bool,
    eof: bool,
}

struct Frame {
    timestamp: f64,
    raw: AVFrame,
    image: Option<SceneImage>,
}

impl Decoder {
    pub fn open(path: &Path, media: Media, seek_s: f64) -> Result<Self, String> {
        let mut input = open_input(path)?;
        let mut sought = false;
        if seek_s > 0.0 {
            let base = media.time_base;
            let timestamp = (seek_s + media.origin_s) * f64::from(base.den) / f64::from(base.num);
            if input
                .seek(
                    media.stream_index as i32,
                    timestamp.floor() as i64,
                    ffi::AVSEEK_FLAG_BACKWARD as i32,
                )
                .is_ok()
            {
                sought = true;
            } else {
                // Some MP4/MOV files have no usable seek index. Decode sequentially.
                input = open_input(path)?;
            }
        }
        let mut codec = {
            let streams = input.streams();
            let stream = streams.get(media.stream_index).ok_or("Video stream disappeared")?;
            let params = stream.codecpar();
            let decoder = AVCodec::find_decoder(params.codec_id)
                .ok_or("FFmpeg has no decoder for this video stream")?;
            let mut codec = AVCodecContext::new(&decoder);
            codec.apply_codecpar(&params).map_err(|err| err.to_string())?;
            codec
        };
        codec
            .open(Some(AVDictionary::new_int(c"threads", 1, 0)))
            .map_err(|err| err.to_string())?;
        // A dynamic scaling context uses each decoded frame's pixel format and
        // color metadata. SwsContext takes ownership of the FFmpeg allocation.
        let scaler = NonNull::new(unsafe { ffi::sws_alloc_context() })
            .map(|context| unsafe { SwsContext::from_raw(context) })
            .ok_or("Could not allocate a video scaler")?;
        Ok(Self {
            input,
            codec,
            scaler,
            media,
            sought,
            previous: None,
            next: None,
            draining: false,
            eof: false,
        })
    }

    pub fn can_sample(&self, time_s: f64) -> bool {
        self.previous.as_ref().is_some_and(|frame| {
            frame.timestamp <= time_s && (self.eof || time_s - frame.timestamp <= 1.0)
        })
    }

    pub fn started_after_zero(&self) -> bool {
        self.sought
    }

    pub fn sample(&mut self, time_s: f64) -> Result<Option<SceneImage>, String> {
        loop {
            if self.next.as_ref().is_some_and(|frame| frame.timestamp > time_s) || self.eof {
                break;
            }
            if let Some(frame) = self.next.take() {
                self.previous = Some(frame);
            }
            self.next = self.read_frame()?;
        }
        let Some(frame) = &mut self.previous else {
            return Ok(None);
        };
        if let Some(image) = &frame.image {
            return Ok(Some(image.clone()));
        }
        let image = convert_frame(&mut self.scaler, self.media, &frame.raw)?;
        frame.image = Some(image.clone());
        Ok(Some(image))
    }

    fn read_frame(&mut self) -> Result<Option<Frame>, String> {
        loop {
            match self.codec.receive_frame() {
                Ok(raw) => {
                    let timestamp = seconds(raw.best_effort_timestamp, self.media.time_base)
                        .ok_or("Decoded video frame has no presentation timestamp")?
                        - self.media.origin_s;
                    return Ok(Some(Frame {
                        timestamp,
                        raw,
                        image: None,
                    }));
                },
                Err(RsmpegError::DecoderFlushedError) => {
                    self.eof = true;
                    return Ok(None);
                },
                Err(RsmpegError::DecoderDrainError) if !self.draining => {},
                Err(error) => return Err(error.to_string()),
            }
            let packet = loop {
                match self.input.read_packet().map_err(|err| err.to_string())? {
                    Some(packet) if packet.stream_index == self.media.stream_index as i32 => {
                        break Some(packet);
                    },
                    Some(_) => {}, // Audio, subtitles, and cover art are not video frames.
                    None => break None,
                }
            };
            if let Some(packet) = packet {
                self.codec.send_packet(Some(&packet)).map_err(|err| err.to_string())?;
            } else {
                self.codec.send_packet(None).map_err(|err| err.to_string())?;
                self.draining = true;
            }
        }
    }
}

fn convert_frame(
    scaler: &mut SwsContext,
    media: Media,
    raw: &AVFrame,
) -> Result<SceneImage, String> {
    let [width, height] = media.scaled_size;
    let mut rgba = AVFrame::new();
    rgba.set_format(ffi::AV_PIX_FMT_RGBA);
    rgba.set_width(width as i32);
    rgba.set_height(height as i32);
    // The scaler allocates an aligned frame owned by rgba if needed.
    let status =
        unsafe { ffi::sws_scale_frame(scaler.as_mut_ptr(), rgba.as_mut_ptr(), raw.as_ptr()) };
    if status < 0 {
        return Err(format!(
            "Could not convert decoded video to RGBA: {}",
            RsmpegError::AVError(status)
        ));
    }
    let mut pixels = vec![0; width as usize * height as usize * 4];
    rgba.image_copy_to_buffer(&mut pixels, 1).map_err(|error| error.to_string())?;
    Ok(orient_pixels(media, pixels))
}

fn orient_pixels(media: Media, source: Vec<u8>) -> SceneImage {
    let orientation = media.orientation;
    let [width, height] = media.metadata.display_size;
    if !orientation.transpose && !orientation.flip_x && !orientation.flip_y {
        return SceneImage::from_rgba8(width, height, source);
    }
    let mut pixels = vec![0; source.len()];
    let [width_px, height_px] = [width as usize, height as usize];
    let source_width = media.scaled_size[0] as usize;
    for (destination, rgba) in pixels.chunks_exact_mut(4).enumerate() {
        let (mut x, mut y) = (destination % width_px, destination / width_px);
        if orientation.flip_x {
            x = width_px - 1 - x;
        }
        if orientation.flip_y {
            y = height_px - 1 - y;
        }
        let index = if orientation.transpose {
            x * source_width + y
        } else {
            y * source_width + x
        } * 4;
        rgba.copy_from_slice(&source[index..index + 4]);
    }
    SceneImage::from_rgba8(width, height, pixels)
}
