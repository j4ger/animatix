//! Build and stream a mixed audio track directly into the video container.

use super::ExportWarning;
use std::ffi::{CStr, CString};
use std::ops::Range;

use animatix::timeline::AudioSegment;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avfilter::{AVFilter, AVFilterContextMut, AVFilterGraph};
use rsmpeg::avformat::{AVFormatContextInput, AVFormatContextOutput};
use rsmpeg::error::RsmpegError;
use rsmpeg::ffi::{self, AVRational};

use super::ExportError;

struct AudioInput<'a> {
    segment: &'a AudioSegment,
    path: CString,
    stream_index: usize,
    duration_s: Option<f64>,
}

impl<'a> AudioInput<'a> {
    fn open(segment: &'a AudioSegment) -> Result<Self, ExportError> {
        let path = CString::new(segment.source.as_str())?;
        let input = AVFormatContextInput::open(&path).map_err(media_error)?;
        let streams = input.streams();
        let (stream_index, stream) = streams
            .iter()
            .enumerate()
            .find(|(_, stream)| stream.codecpar().codec_type == ffi::AVMEDIA_TYPE_AUDIO)
            .ok_or_else(|| ExportError::VideoEncode("No audio stream found".into()))?;
        let duration_s = if stream.duration > 0 && stream.time_base.den > 0 {
            Some(
                stream.duration as f64 * f64::from(stream.time_base.num)
                    / f64::from(stream.time_base.den),
            )
        } else {
            (input.duration > 0 && streams.len() == 1)
                .then(|| input.duration as f64 / f64::from(ffi::AV_TIME_BASE))
        };
        let duration_s = match (segment.duration_s, duration_s) {
            (Some(trim), Some(media)) => Some(trim.min(media)),
            (trim, media) => trim.or(media),
        };
        Ok(Self {
            segment,
            path,
            stream_index,
            duration_s,
        })
    }
}

struct AudioFormat {
    encoder: &'static CStr,
    sample_format: ffi::AVSampleFormat,
    sample_format_name: &'static str,
    sample_rate: i32,
}

impl AudioFormat {
    fn for_container(webm: bool) -> Self {
        if webm {
            Self {
                encoder: c"libopus",
                sample_format: ffi::AV_SAMPLE_FMT_FLT,
                sample_format_name: "flt",
                sample_rate: 48_000,
            }
        } else {
            Self {
                encoder: c"aac",
                sample_format: ffi::AV_SAMPLE_FMT_FLTP,
                sample_format_name: "fltp",
                sample_rate: 44_100,
            }
        }
    }

    fn samples(&self, seconds: f64) -> i64 {
        (seconds.max(0.0) * f64::from(self.sample_rate)).round() as i64
    }
}

pub(super) struct AudioEncoder {
    graph: AVFilterGraph,
    codec: AVCodecContext,
    stream_index: i32,
    submitted_samples: i64,
    finished: bool,
}

impl AudioEncoder {
    /// Prepare the filter graph before the first frame and register its output stream
    /// before the muxer writes its header.
    pub(super) fn prepare(
        container: &mut AVFormatContextOutput,
        segments: &[AudioSegment],
        range: Range<f64>,
        webm: bool,
        warn_on_cutoff: bool,
        warn: impl Fn(ExportWarning),
    ) -> Result<Option<Self>, ExportError> {
        let mut inputs = Vec::with_capacity(segments.len());
        for segment in segments {
            if segment.start_time_s >= range.end {
                if warn_on_cutoff {
                    warn(ExportWarning::AudioStartsAfterExport {
                        source: segment.source.to_string(),
                        cutoff_s: range.end,
                        start_s: segment.start_time_s,
                    });
                }
                continue;
            }
            if segment
                .duration_s
                .is_some_and(|duration| segment.start_time_s + duration <= range.start)
                && range.start > 0.0
            {
                continue;
            }
            let input = AudioInput::open(segment).map_err(|err| input_error(segment, err))?;
            if let Some(end) = input.duration_s.map(|duration| segment.start_time_s + duration) {
                if range.start > 0.0 && end <= range.start {
                    continue;
                }
                if warn_on_cutoff && end > range.end + 0.001 {
                    warn(ExportWarning::AudioCutOff {
                        source: segment.source.to_string(),
                        cutoff_s: range.end,
                        end_s: end,
                    });
                }
            }
            inputs.push(input);
        }
        if inputs.is_empty() {
            return Ok(None);
        }

        let format = AudioFormat::for_container(webm);
        let encoder = AVCodec::find_encoder_by_name(format.encoder).ok_or_else(|| {
            ExportError::VideoEncode(format!("Audio encoder {:?} is unavailable", format.encoder))
        })?;
        let mut codec = AVCodecContext::new(&encoder);
        codec.set_ch_layout(ffi::AV_CHANNEL_LAYOUT_STEREO);
        codec.set_sample_rate(format.sample_rate);
        codec.set_sample_fmt(format.sample_format);
        codec.set_time_base(AVRational {
            num: 1,
            den: format.sample_rate,
        });
        codec.set_bit_rate(128_000);
        if container.oformat().flags & ffi::AVFMT_GLOBALHEADER as i32 != 0 {
            codec.set_flags(codec.flags | ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        }
        codec.open(None).map_err(media_error)?;
        let graph = mix_graph(&inputs, &format, range, codec.frame_size)?;
        let mut stream = container.new_stream();
        stream.set_time_base(codec.time_base);
        stream.set_codecpar(codec.extract_codecpar());
        Ok(Some(Self {
            graph,
            codec,
            stream_index: stream.index,
            submitted_samples: 0,
            finished: false,
        }))
    }

    /// Keep audio roughly alongside video to bound libavformat's interleave queue.
    pub(super) fn advance(
        &mut self,
        container: &mut AVFormatContextOutput,
        until_s: f64,
    ) -> Result<(), ExportError> {
        let until_sample = (until_s * f64::from(self.codec.sample_rate)).ceil() as i64;
        while !self.finished && self.submitted_samples < until_sample {
            let result = self
                .graph
                .get_filter(c"output")
                .expect("mix graph owns its sink")
                .buffersink_get_frame(None);
            let mut frame = match result {
                Ok(frame) => frame,
                Err(RsmpegError::BufferSinkEofError) => {
                    self.finished = true;
                    break;
                },
                Err(err) => return Err(media_error(err)),
            };
            frame.set_pts(self.submitted_samples);
            self.submitted_samples += i64::from(frame.nb_samples);
            self.codec.send_frame(Some(&frame)).map_err(media_error)?;
            self.write_packets(container)?;
        }
        Ok(())
    }

    pub(super) fn finish(
        &mut self,
        container: &mut AVFormatContextOutput,
    ) -> Result<(), ExportError> {
        self.advance(container, f64::INFINITY)?;
        self.codec.send_frame(None).map_err(media_error)?;
        self.write_packets(container)
    }

    fn write_packets(&mut self, container: &mut AVFormatContextOutput) -> Result<(), ExportError> {
        let stream_time_base = container.streams()[self.stream_index as usize].time_base;
        loop {
            match self.codec.receive_packet() {
                Ok(mut packet) => {
                    packet.set_stream_index(self.stream_index);
                    packet.rescale_ts(self.codec.time_base, stream_time_base);
                    container.interleaved_write_frame(&mut packet).map_err(media_error)?;
                },
                Err(RsmpegError::EncoderDrainError | RsmpegError::EncoderFlushedError) => {
                    return Ok(());
                },
                Err(err) => return Err(media_error(err)),
            }
        }
    }
}

fn mix_graph(
    inputs: &[AudioInput<'_>],
    format: &AudioFormat,
    range: Range<f64>,
    frame_size: i32,
) -> Result<AVFilterGraph, ExportError> {
    let graph = AVFilterGraph::new();
    configure_mix_graph(&graph, inputs, format, range, frame_size)?;
    Ok(graph)
}

fn configure_mix_graph(
    graph: &AVFilterGraph,
    inputs: &[AudioInput<'_>],
    format: &AudioFormat,
    range: Range<f64>,
    frame_size: i32,
) -> Result<(), ExportError> {
    // The video renderer has its own parallel workers. One filter thread keeps
    // parallel exports from creating a full audio filter pool per job.
    set_graph_threads(graph)?;
    let args = format!("inputs={}:duration=longest:normalize=0", inputs.len());
    let mut mix = filter(graph, c"amix", c"mix", &args)?;
    for (index, input) in inputs.iter().enumerate() {
        add_input(graph, input, index, format, &mut mix)
            .map_err(|err| input_error(input.segment, err))?;
    }

    let mut stereo = filter(
        graph,
        c"aformat",
        c"output_format",
        &format!(
            "sample_fmts={}:sample_rates={}:channel_layouts=stereo",
            format.sample_format_name, format.sample_rate
        ),
    )?;
    let mut pad = filter(graph, c"apad", c"output_pad", "")?;
    let mut trim = filter(
        graph,
        c"atrim",
        c"output_trim",
        // Crop the mixed samples on the composition clock. The encoder assigns
        // timestamps from sample zero after trimming.
        &format!(
            "start_sample={}:end_sample={}",
            format.samples(range.start),
            format.samples(range.end)
        ),
    )?;
    let mut sink = filter(graph, c"abuffersink", c"output", "")?;
    mix.link(0, &mut stereo, 0).map_err(media_error)?;
    stereo.link(0, &mut pad, 0).map_err(media_error)?;
    pad.link(0, &mut trim, 0).map_err(media_error)?;
    trim.link(0, &mut sink, 0).map_err(media_error)?;
    graph.config().map_err(media_error)?;
    if frame_size > 0 {
        sink.buffersink_set_frame_size(frame_size as u32);
    }
    Ok(())
}

fn add_input(
    graph: &AVFilterGraph,
    input: &AudioInput<'_>,
    index: usize,
    format: &AudioFormat,
    mix: &mut AVFilterContextMut<'_>,
) -> Result<(), ExportError> {
    let name = |stage| CString::new(format!("{stage}_{index}"));
    let movie = definition(c"amovie")?;
    let mut source = graph
        .alloc_filter_context(&movie, &name("source")?)
        .ok_or_else(|| ExportError::VideoEncode("Could not allocate audio source filter".into()))?;
    // A filename is a filter option value, never interpolated into filter syntax.
    source.opt_set(c"filename", &input.path).map_err(media_error)?;
    source
        .opt_set_int(c"stream_index", input.stream_index as i64)
        .map_err(media_error)?;
    source.init_str(None).map_err(media_error)?;

    let segment = input.segment;
    let mut stereo = filter(
        graph,
        c"aformat",
        &name("format")?,
        &format!("sample_fmts=fltp:sample_rates={}:channel_layouts=stereo", format.sample_rate),
    )?;
    let mut timestamps = filter(graph, c"asetpts", &name("timestamps")?, "expr=N/SR/TB")?;
    let mut trim = filter(
        graph,
        c"atrim",
        &name("trim")?,
        &segment
            .duration_s
            .map(|duration| format!("end_sample={}", format.samples(duration)))
            .unwrap_or_default(),
    )?;
    let mut volume = filter(graph, c"volume", &name("volume")?, &gain_args(segment))?;
    let mut delay = filter(
        graph,
        c"adelay",
        &name("delay")?,
        &format!("delays={}S:all=1", format.samples(segment.start_time_s)),
    )?;
    source.link(0, &mut stereo, 0).map_err(media_error)?;
    stereo.link(0, &mut timestamps, 0).map_err(media_error)?;
    timestamps.link(0, &mut trim, 0).map_err(media_error)?;
    trim.link(0, &mut volume, 0).map_err(media_error)?;
    volume.link(0, &mut delay, 0).map_err(media_error)?;
    delay.link(0, mix, index as u32).map_err(media_error)
}

fn gain_args(segment: &AudioSegment) -> String {
    let Some(keys) = &segment.volume_envelope else {
        return format!("volume={}", segment.volume);
    };
    let mut expression = keys.last().map_or(segment.volume, |(_, gain)| *gain).to_string();
    for pair in keys.windows(2).rev() {
        let [(first, low), (last, high)] = pair else {
            unreachable!("window has two keys")
        };
        let start = first - segment.start_time_s;
        let end = last - segment.start_time_s;
        if end > start && end > 0.0 {
            let width = end - start;
            expression = format!(
                "if(lt(t,{end:.9}),{low}+({high}-{low})*clip((t-{start:.9})/{width:.9},0,1),{expression})"
            );
        }
    }
    format!("volume={expression}:eval=frame")
}

fn set_graph_threads(graph: &AVFilterGraph) -> Result<(), ExportError> {
    // SAFETY: graph points to a live AVFilterGraph with an AVClass. This runs
    // before any filters are allocated or borrow the graph.
    let result =
        unsafe { ffi::av_opt_set_int(graph.as_ptr().cast_mut().cast(), c"threads".as_ptr(), 1, 0) };
    if result < 0 {
        return Err(media_error(RsmpegError::AVError(result)));
    }
    Ok(())
}

fn definition(name: &CStr) -> Result<rsmpeg::avfilter::AVFilterRef<'static>, ExportError> {
    AVFilter::get_by_name(name)
        .ok_or_else(|| ExportError::VideoEncode(format!("Audio filter {name:?} is unavailable")))
}

fn filter<'a>(
    graph: &'a AVFilterGraph,
    filter: &CStr,
    name: &CStr,
    options: &str,
) -> Result<AVFilterContextMut<'a>, ExportError> {
    let options = CString::new(options)?;
    let filter = definition(filter)?;
    graph.create_filter_context(&filter, name, Some(&options)).map_err(media_error)
}

fn media_error(err: RsmpegError) -> ExportError {
    ExportError::VideoEncode(format!("Audio processing failed: {err}"))
}

fn input_error(segment: &AudioSegment, error: ExportError) -> ExportError {
    ExportError::VideoEncode(format!("Audio '{}': {error}", segment.source))
}

#[cfg(test)]
mod tests;
