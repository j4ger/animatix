//! Fixtures shared by the crate's GPU-backed unit tests.
//!
//! Compiled only under `#[cfg(test)]`, so nothing here reaches a consumer of
//! the library.

/// Request a headless wgpu device and queue, or `None` when no adapter is
/// available.
///
/// `label` names the device in backend logs, so a failing suite can tell which
/// test file created it. The request forces the fallback adapter so it works
/// on CI and headless boxes; callers treat `None` as "skip the GPU
/// assertions".
pub(crate) async fn create_headless_device(label: &str) -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: true,
            apply_limit_buckets: false,
        })
        .await
        .ok()?;
    let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some(label),
            required_features: wgpu::Features::empty(),
            required_limits: needed_limits,
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .ok()?;
    Some((device, queue))
}
