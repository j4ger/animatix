use super::FullscreenBlitPipeline;
use std::sync::Arc;

const SIZE: u32 = 64;

fn target(
    device: &wgpu::Device,
    usage: wgpu::TextureUsages,
    label: &str,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn clear_black(device: &wgpu::Device, queue: &wgpu::Queue, view: &wgpu::TextureView) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blit immutable uniform clear encoder"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blit immutable uniform clear pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    queue.submit([encoder.finish()]);
}

fn pixel(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> [u8; 4] {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blit immutable uniform readback"),
        size: (SIZE * SIZE * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blit immutable uniform readback encoder"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIZE * 4),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).ok();
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .expect("poll blit readback");
    rx.recv().expect("blit readback callback").expect("map blit readback");
    let mapped = slice.get_mapped_range().expect("mapped blit readback");
    let offset = ((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize;
    let value = [
        mapped[offset],
        mapped[offset + 1],
        mapped[offset + 2],
        mapped[offset + 3],
    ];
    drop(mapped);
    buffer.unmap();
    value
}

#[test]
fn opaque_suffix_uses_immutable_group_after_mutable_same_source_encoder() {
    let Some((device, queue)) = pollster::block_on(crate::test_support::create_headless_device(
        "Animatix immutable blit uniform test",
    )) else {
        crate::testing::skip_if_no_gpu();
        return;
    };
    device.on_uncaptured_error(Arc::new(|error| {
        panic!("uncaptured wgpu error in immutable blit test: {error}");
    }));
    let pipeline = FullscreenBlitPipeline::new(&device, wgpu::TextureFormat::Rgba8Unorm);
    let (source, source_view) = target(
        &device,
        wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        "mutable then opaque source",
    );
    let (staged_output, staged_view) = target(
        &device,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        "mutable staged output",
    );
    let (opaque_output, opaque_view) = target(
        &device,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        "immutable opaque output",
    );

    let rgba = [255u8, 0, 0, 255].repeat((SIZE * SIZE) as usize);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 4),
            rows_per_image: Some(SIZE),
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    clear_black(&device, &queue, &staged_view);
    clear_black(&device, &queue, &opaque_view);

    // Leave the mutable uniform at alpha=.25 after a real staged draw.
    let mut staged_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mutable alpha encoder"),
    });
    pipeline.blit_with_encoder(
        &device,
        &queue,
        &mut staged_encoder,
        &source_view,
        &staged_view,
        SIZE,
        SIZE,
        0.25,
    );
    // The same source view has a different cache key and binds the immutable
    // alpha=1 buffer while a pending draw will write .25 to the mutable buffer.
    let mut opaque_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("opaque suffix encoder"),
    });
    pipeline.blit_opaque_with_encoder(&device, &mut opaque_encoder, &source_view, &opaque_view);
    // The mutable parameter copy executes after both encoders have been
    // recorded, immediately before the opaque command buffer is submitted.
    queue.submit([staged_encoder.finish()]);
    queue.submit([opaque_encoder.finish()]);

    let staged_pixel = pixel(&device, &queue, &staged_output);
    assert!(
        (60..=68).contains(&staged_pixel[0]) && staged_pixel[1] <= 2 && staged_pixel[2] <= 2,
        "the mutable path should have blended at alpha .25, got {staged_pixel:?}"
    );

    let opaque_pixel = pixel(&device, &queue, &opaque_output);
    assert_eq!(
        opaque_pixel,
        [255, 0, 0, 255],
        "opaque suffix must not reuse the mutable same-source bind group"
    );
    let immutable = pipeline
        .opaque_alpha1_buffer
        .get()
        .expect("first opaque suffix initializes its constant buffer");
    assert_eq!(immutable.size(), super::BLIT_PARAMS_SIZE);
    assert_eq!(immutable.usage(), wgpu::BufferUsages::UNIFORM);
}
