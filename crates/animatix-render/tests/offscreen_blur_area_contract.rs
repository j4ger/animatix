#[path = "../examples/offscreen_blur_area/workload.rs"]
mod workload;

use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use animatix_render::offscreen::{OffscreenRenderer, RenderedFrame};

#[test]
fn device_classification_does_not_mix_software_or_unknown_with_hardware() {
    assert_eq!(workload::device_class(wgpu::DeviceType::Cpu), "software");
    assert_eq!(workload::device_class(wgpu::DeviceType::Other), "unknown_unqualified");
    assert_eq!(workload::device_class(wgpu::DeviceType::VirtualGpu), "virtual_unqualified");
    assert_eq!(workload::device_class(wgpu::DeviceType::DiscreteGpu), "hardware_reported");
}

#[test]
fn six_sources_build_with_expected_filter_path() {
    assert_eq!(workload::WARMUP, 4);
    for size in workload::SIZES {
        for blur in [false, true] {
            let timeline = workload::build(&workload::source(size, blur)).expect("fixture");
            assert_eq!(timeline.needs_filter_backend(), blur);
        }
    }
}

#[test]
fn offscreen_six_cases_and_guard_fault_injection() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(renderer) => renderer,
        Err(_) => {
            animatix_render::testing::skip_if_no_gpu();
            return;
        },
    };
    let dims = SceneDimensions {
        width: workload::WIDTH,
        height: workload::WIDTH,
    };
    for size in workload::SIZES {
        let mut plain = None;
        for blur in [false, true] {
            let timeline = workload::build(&workload::source(size, blur)).expect("fixture");
            let pending = renderer
                .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
                .expect("submit");
            let frame = renderer.wait_frame(pending).expect("complete");
            workload::guard(&frame, size, blur).expect("actual output guard");
            assert!(
                workload::guard(&frame, size, !blur).is_err(),
                "missing/unexpected blur must fail"
            );
            let wrong_size = if size == 64 { 256 } else { 64 };
            assert!(workload::guard(&frame, wrong_size, blur).is_err());
            if blur {
                let plain: RenderedFrame = plain.take().expect("plain partner");
                assert_ne!(plain.rgba, frame.rgba, "blur cannot be a no-op");
            } else {
                plain = Some(frame);
            }
        }
    }
    let mut black = vec![0; 512 * 512 * 4];
    for pixel in black.as_chunks_mut::<4>().0 {
        pixel[3] = 255;
    }
    let blank = RenderedFrame {
        width: 512,
        height: 512,
        rgba: std::sync::Arc::new(black),
    };
    assert!(workload::guard(&blank, 64, false).is_err());
    let malformed = RenderedFrame {
        width: 512,
        height: 512,
        rgba: std::sync::Arc::new(vec![255; 4]),
    };
    assert!(workload::guard(&malformed, 64, false).is_err());
}

#[test]
fn source_bytes_are_frozen_v1() {
    let expected: [u64; 6] = [
        0xedac2cd9f4e6d852,
        0x4f0f2b8fdb9fef2f,
        0x6253b36350a12ec8,
        0xe9b72a154240883d,
        0x733300f90038c1e8,
        0x37fcbd4b23e8141d,
    ];
    let mut index = 0;
    for size in workload::SIZES {
        for blur in [false, true] {
            let hash =
                workload::source(size, blur).bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
                });
            assert_eq!(hash, expected[index]);
            index += 1;
        }
    }
}
