//! Fixtures shared by the crate's GPU-backed unit tests.

/// Request a fallback device for headless tests; return `None` if unavailable.
pub(crate) async fn create_headless_device(label: &str) -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let Ok(adapter) = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: true,
            apply_limit_buckets: false,
        })
        .await
    else {
        crate::testing::skip_if_no_gpu();
        return None;
    };
    let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let Ok((device, queue)) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some(label),
            required_features: wgpu::Features::empty(),
            required_limits: needed_limits,
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
    else {
        crate::testing::skip_if_no_gpu();
        return None;
    };
    Some((device, queue))
}
