//! Pixel parity and renderer self-noise checks for the native presentation driver.

use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc;

use super::{
    Fixture, Mode, RendererCore, TargetPair, create_target_pair, draw_frame, error, raster_dims,
};

const MISMATCH_EXAMPLE_LIMIT: usize = 64;
const COMPARISONS: [(&str, usize, usize); 6] = [
    ("separate_repeat_control", 0, 1),
    ("merged_repeat_control", 2, 3),
    ("separate_1_vs_merged_1", 0, 2),
    ("separate_1_vs_merged_2", 0, 3),
    ("separate_2_vs_merged_1", 1, 2),
    ("separate_2_vs_merged_2", 1, 3),
];

#[derive(Clone, Copy)]
struct VerifyStep {
    fixture: usize,
    sample: usize,
    scale: f32,
    present_size: (u32, u32),
}

pub(super) struct VerificationResult {
    pub(super) passed: bool,
    pub(super) checks: Vec<serde_json::Value>,
    pub(super) raw_pixels: Vec<u8>,
    pub(super) raw_layout: Vec<serde_json::Value>,
}

pub(super) fn verify_resized_image_sequence(
    fixtures: &[Fixture],
    present_format: wgpu::TextureFormat,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<VerificationResult, Box<dyn Error>> {
    let sequence = [
        VerifyStep {
            fixture: 0,
            sample: 0,
            scale: 0.85,
            present_size: (1052, 592),
        },
        VerifyStep {
            fixture: 1,
            sample: 0,
            scale: 0.85,
            present_size: (1052, 592),
        },
        VerifyStep {
            fixture: 0,
            sample: 1,
            scale: 0.5,
            present_size: (640, 360),
        },
        VerifyStep {
            fixture: 1,
            sample: 1,
            scale: 1.0,
            present_size: (1280, 720),
        },
        VerifyStep {
            fixture: 0,
            sample: 2,
            scale: 0.85,
            present_size: (1052, 592),
        },
    ];
    let separate_first =
        render_verification(Mode::Separate, fixtures, &sequence, present_format, device, queue)?;
    let merged_first =
        render_verification(Mode::Merged, fixtures, &sequence, present_format, device, queue)?;
    let separate_second =
        render_verification(Mode::Separate, fixtures, &sequence, present_format, device, queue)?;
    let merged_second =
        render_verification(Mode::Merged, fixtures, &sequence, present_format, device, queue)?;
    let runs = [
        ("separate_repeat_1", Mode::Separate, 1, &separate_first),
        ("separate_repeat_2", Mode::Separate, 2, &separate_second),
        ("merged_repeat_1", Mode::Merged, 1, &merged_first),
        ("merged_repeat_2", Mode::Merged, 2, &merged_second),
    ];
    let frames_by_run: [&[Vec<u8>]; 4] = [
        &separate_first,
        &separate_second,
        &merged_first,
        &merged_second,
    ];
    let mut raw_pixels = Vec::new();
    let mut raw_layout = Vec::new();
    append_raw_outputs(&runs, &sequence, fixtures, &mut raw_pixels, &mut raw_layout);

    let mut checks = Vec::with_capacity(sequence.len());
    let mut passed = true;
    for (frame, step) in sequence.iter().enumerate() {
        let expected_len = (step.present_size.0 as usize)
            .saturating_mul(step.present_size.1 as usize)
            .saturating_mul(4);
        let all_run_lengths_match_dimensions =
            frames_by_run.iter().all(|run| run[frame].len() == expected_len);
        let mut comparisons = Vec::with_capacity(COMPARISONS.len());
        let mut frame_passed = all_run_lengths_match_dimensions;
        for (name, left_run, right_run) in COMPARISONS {
            let difference = pixel_difference(
                &frames_by_run[left_run][frame],
                &frames_by_run[right_run][frame],
                step.present_size.0,
                step.present_size.1,
            );
            let equal = difference.lengths_equal && difference.mismatched_bytes == 0;
            frame_passed &= equal;
            comparisons.push(serde_json::json!({
                "name": name,
                "left_run": runs[left_run].0,
                "right_run": runs[right_run].0,
                "equal": equal,
                "stats": difference.to_json(),
            }));
        }
        passed &= frame_passed;
        let fixture = &fixtures[step.fixture];
        let raster = raster_dims(fixture.dims, step.scale);
        let sample = &fixture.samples[step.sample];
        checks.push(serde_json::json!({
            "frame": frame,
            "fixture": fixture.name,
            "time_s": sample.time_s,
            "raster_scale": step.scale,
            "raster": [raster.0, raster.1],
            "presentation": [step.present_size.0, step.present_size.1],
            "expected_byte_length": expected_len,
            "all_run_lengths_match_dimensions": all_run_lengths_match_dimensions,
            "comparisons": comparisons,
        }));
    }

    Ok(VerificationResult {
        passed,
        checks,
        raw_pixels,
        raw_layout,
    })
}

fn append_raw_outputs(
    runs: &[(&str, Mode, usize, &Vec<Vec<u8>>); 4],
    sequence: &[VerifyStep],
    fixtures: &[Fixture],
    raw_pixels: &mut Vec<u8>,
    raw_layout: &mut Vec<serde_json::Value>,
) {
    for (run_name, mode, repeat, outputs) in runs {
        let mut frames = Vec::with_capacity(outputs.len());
        for (frame, pixels) in outputs.iter().enumerate() {
            let step = sequence[frame];
            let fixture = &fixtures[step.fixture];
            let offset = raw_pixels.len();
            raw_pixels.extend_from_slice(pixels);
            frames.push(serde_json::json!({
                "frame": frame,
                "fixture": fixture.name,
                "time_s": fixture.samples[step.sample].time_s,
                "width": step.present_size.0,
                "height": step.present_size.1,
                "expected_byte_length": u64::from(step.present_size.0)
                    * u64::from(step.present_size.1)
                    * 4,
                "byte_offset": offset,
                "byte_length": pixels.len(),
            }));
        }
        raw_layout.push(serde_json::json!({
            "run": run_name,
            "mode": mode.label(),
            "repeat": repeat,
            "frames": frames,
        }));
    }
}

fn render_verification(
    mode: Mode,
    fixtures: &[Fixture],
    sequence: &[VerifyStep],
    present_format: wgpu::TextureFormat,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let mut core = RendererCore::new(device, queue)?;
    let mut target_sets = HashMap::<(u32, u32, u32, u32), [TargetPair; 2]>::new();
    let mut scratch = animatix::scene::Scene::new();
    let mut outputs = Vec::with_capacity(sequence.len());
    for (index, step) in sequence.iter().enumerate() {
        let fixture = &fixtures[step.fixture];
        let sample = &fixture.samples[step.sample];
        let raster = raster_dims(fixture.dims, step.scale);
        let key = (raster.0, raster.1, step.present_size.0, step.present_size.1);
        let targets = target_sets.entry(key).or_insert_with(|| {
            [
                create_target_pair(device, raster, step.present_size, present_format),
                create_target_pair(device, raster, step.present_size, present_format),
            ]
        });
        let target = &targets[index % 2];
        draw_frame(
            &mut core,
            &mut scratch,
            sample,
            step.scale,
            raster,
            step.present_size,
            present_format,
            target,
            mode,
            device,
            queue,
        )?;
        outputs.push(readback_texture(
            device,
            queue,
            &target.present_texture,
            target.present_size,
        )?);
    }
    Ok(outputs)
}

fn readback_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    size: [u32; 2],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let row_bytes = size[0] * 4;
    let padded_row_bytes = row_bytes.div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Animatix Web Present Verification Readback"),
        size: u64::from(padded_row_bytes) * u64::from(size[1]),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Animatix Web Present Verification Copy"),
    });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(size[1]),
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    receiver.recv()??;
    let mapped = slice.get_mapped_range().map_err(|err| error(format!("map range: {err}")))?;
    let mut pixels = Vec::with_capacity((row_bytes * size[1]) as usize);
    for row in 0..size[1] as usize {
        let start = row * padded_row_bytes as usize;
        pixels.extend_from_slice(&mapped[start..start + row_bytes as usize]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(pixels)
}

struct PixelDifference {
    lengths_equal: bool,
    left_bytes: usize,
    right_bytes: usize,
    mismatched_bytes: usize,
    mismatched_pixels: usize,
    max_abs_delta: u8,
    total_abs_delta: u64,
    mismatch_examples: Vec<serde_json::Value>,
}

impl PixelDifference {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "lengths_equal": self.lengths_equal,
            "left_bytes": self.left_bytes,
            "right_bytes": self.right_bytes,
            "mismatched_bytes": self.mismatched_bytes,
            "mismatched_pixels": self.mismatched_pixels,
            "max_abs_delta": self.max_abs_delta,
            "total_abs_delta": self.total_abs_delta,
            "mismatch_examples_limit": MISMATCH_EXAMPLE_LIMIT,
            "mismatch_examples_truncated": self.mismatched_pixels > self.mismatch_examples.len(),
            "mismatch_examples": &self.mismatch_examples,
        })
    }
}

fn pixel_difference(left: &[u8], right: &[u8], width: u32, height: u32) -> PixelDifference {
    let mut difference = PixelDifference {
        lengths_equal: left.len() == right.len(),
        left_bytes: left.len(),
        right_bytes: right.len(),
        mismatched_bytes: 0,
        mismatched_pixels: 0,
        max_abs_delta: 0,
        total_abs_delta: 0,
        mismatch_examples: Vec::new(),
    };
    let common_len = left.len().min(right.len());
    for index in 0..common_len {
        let delta = left[index].abs_diff(right[index]);
        if delta != 0 {
            difference.mismatched_bytes += 1;
            difference.max_abs_delta = difference.max_abs_delta.max(delta);
            difference.total_abs_delta += u64::from(delta);
        }
    }
    let extra_bytes = left.len().abs_diff(right.len());
    difference.mismatched_bytes += extra_bytes;
    if extra_bytes > 0 {
        difference.max_abs_delta = u8::MAX;
        difference.total_abs_delta += u64::from(u8::MAX) * extra_bytes as u64;
    }

    let pixel_count = left.len().max(right.len()).div_ceil(4);
    for pixel_index in 0..pixel_count {
        let start = pixel_index * 4;
        let left_pixel: Vec<Option<u8>> =
            (0..4).map(|channel| left.get(start + channel).copied()).collect();
        let right_pixel: Vec<Option<u8>> =
            (0..4).map(|channel| right.get(start + channel).copied()).collect();
        if left_pixel == right_pixel {
            continue;
        }
        difference.mismatched_pixels += 1;
        if difference.mismatch_examples.len() < MISMATCH_EXAMPLE_LIMIT {
            let differing_byte_slots: Vec<usize> =
                (0..4).filter(|channel| left_pixel[*channel] != right_pixel[*channel]).collect();
            let x = pixel_index as u32 % width.max(1);
            let y = pixel_index as u32 / width.max(1);
            difference.mismatch_examples.push(serde_json::json!({
                "x": x,
                "y": y,
                "outside_image": y >= height,
                "left_pixel_bytes": left_pixel,
                "right_pixel_bytes": right_pixel,
                "differing_byte_slots": differing_byte_slots,
            }));
        }
    }
    difference
}
