//! Small data-only raster encoder facade; encoding never loads implicitly.

struct EncodeRequest {
    rgba: *const u8,
    rgba_len: usize,
    width: u32,
    height: u32,
    output: *mut Option<Result<Vec<u8>, String>>,
}

fn png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    if width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .map(|size| size as usize)
        != Some(rgba.len())
    {
        return Err("RGBA buffer does not match image dimensions".to_string());
    }
    let mut output = Vec::new();
    image::codecs::png::PngEncoder::new(&mut output)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|error| error.to_string())?;
    Ok(output)
}

/// Encode one request. The direct `png` call keeps the implementation
/// reachable from the wasm-split entrypoint.
unsafe fn dispatch_request(request: *mut EncodeRequest) -> u32 {
    // SAFETY: callers provide a live, exclusively borrowed stack request.
    let request = unsafe { &mut *request };
    // SAFETY: the caller's RGBA slice remains live for this synchronous call.
    let rgba = unsafe { std::slice::from_raw_parts(request.rgba, request.rgba_len) };
    let result = png(rgba, request.width, request.height);
    // SAFETY: caller provides a unique output slot that remains live until this
    // call returns. The produced Vec owns its allocation and escapes via slot.
    unsafe { *request.output = Some(result) };
    0
}

/// wasm-split entrypoint for synchronous PNG encoding.
#[cfg(target_arch = "wasm32")]
fn dispatch_encode(request_address: u32) -> u32 {
    // SAFETY: caller passes the stack request address and keeps input/output
    // storage live and exclusively borrowed through this synchronous dispatch.
    unsafe { dispatch_request(request_address as usize as *mut EncodeRequest) }
}

#[cfg(target_arch = "wasm32")]
static LOADER: wasm_split::LazyLoader<u32, u32> =
    wasm_split::lazy_loader!(extern "encode" fn dispatch_encode(request_address: u32) -> u32);

/// Prepare the encoding capability once, outside the render loop.
#[cfg(target_arch = "wasm32")]
pub async fn activate() -> Result<(), String> {
    if LOADER.load().await {
        Ok(())
    } else {
        Err("raster encoding capability could not load".to_string())
    }
}

/// Encode already-rendered tight RGBA rows as PNG bytes.
pub fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut result = None;
    let mut request = EncodeRequest {
        rgba: rgba.as_ptr(),
        rgba_len: rgba.len(),
        width,
        height,
        output: &mut result,
    };
    #[cfg(target_arch = "wasm32")]
    {
        let address = (&mut request as *mut EncodeRequest) as usize as u32;
        if LOADER.call(address).map_err(|error| error.to_string())? != 0 {
            return Err("unknown raster operation".to_string());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // SAFETY: input and output pointers refer to this call's live borrows;
        // dispatch is synchronous and does not retain either pointer.
        if unsafe { dispatch_request(&mut request) } != 0 {
            return Err("unknown raster operation".to_string());
        }
    }
    result.expect("raster dispatch must write its result")
}
