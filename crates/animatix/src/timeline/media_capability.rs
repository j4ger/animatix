//! Media decoding behind a cold synchronous dispatch entrypoint.

#[cfg(any(feature = "image-decode", feature = "svg"))]
struct Request {
    operation: u32,
    input: *const u8,
    input_len: usize,
    output: *mut (),
}

#[cfg(any(feature = "image-decode", feature = "svg"))]
const IMAGE: u32 = 0;
#[cfg(any(feature = "image-decode", feature = "svg"))]
const SVG: u32 = 1;

/// Decode/parse one request. The direct implementation calls keep these bodies
/// reachable from the wasm-split entrypoint.
#[cfg(any(feature = "image-decode", feature = "svg"))]
unsafe fn dispatch_request(request: *mut Request) -> u32 {
    // SAFETY: callers guarantee `request` points to an initialized, exclusively
    // borrowed stack request that remains live for this synchronous call.
    let request = unsafe { &mut *request };
    match request.operation {
        #[cfg(feature = "image-decode")]
        IMAGE => {
            // SAFETY: caller provides a valid byte slice and a uniquely borrowed
            // `Option<Result<SceneImage, String>>` output slot for this call.
            let bytes = unsafe { std::slice::from_raw_parts(request.input, request.input_len) };
            let result = super::image::decode_bytes(bytes);
            let output = unsafe {
                &mut *(request.output as *mut Option<Result<super::image::SceneImage, String>>)
            };
            *output = Some(result);
            0
        },
        #[cfg(feature = "svg")]
        SVG => {
            // SAFETY: caller provides a valid UTF-8 source slice and a uniquely
            // borrowed result slot for this call; neither pointer is retained.
            let bytes = unsafe { std::slice::from_raw_parts(request.input, request.input_len) };
            let source = match std::str::from_utf8(bytes) {
                Ok(source) => source,
                Err(error) => {
                    let output = unsafe {
                        &mut *(request.output
                            as *mut Option<Result<Vec<crate::renderer::types::VelloPath>, String>>)
                    };
                    *output = Some(Err(error.to_string()));
                    return 0;
                },
            };
            let result = super::svg::parse_svg_impl(source);
            let output = unsafe {
                &mut *(request.output
                    as *mut Option<Result<Vec<crate::renderer::types::VelloPath>, String>>)
            };
            *output = Some(result);
            0
        },
        _ => 1,
    }
}

/// wasm-split entrypoint: direct calls keep media implementations in the cold
/// module instead of leaving a tiny factory that merely returns fn pointers.
#[cfg(all(target_arch = "wasm32", any(feature = "image-decode", feature = "svg")))]
fn dispatch_media(request_address: u32) -> u32 {
    // SAFETY: caller passes the address of its stack-local request and keeps its
    // input and output storage live and exclusively borrowed until this returns.
    unsafe { dispatch_request(request_address as usize as *mut Request) }
}

#[cfg(all(target_arch = "wasm32", any(feature = "image-decode", feature = "svg")))]
static LOADER: wasm_split::LazyLoader<u32, u32> =
    wasm_split::lazy_loader!(extern "media" fn dispatch_media(request_address: u32) -> u32);

/// Load media code before a prepared document decodes its assets.
#[cfg(target_arch = "wasm32")]
pub async fn activate() -> Result<(), String> {
    #[cfg(any(feature = "image-decode", feature = "svg"))]
    {
        if LOADER.load().await {
            Ok(())
        } else {
            Err("media capability could not load".to_string())
        }
    }
    #[cfg(not(any(feature = "image-decode", feature = "svg")))]
    {
        Err("media capability is not compiled in".to_string())
    }
}

#[cfg(feature = "image-decode")]
pub(super) fn image(bytes: &[u8]) -> Result<super::image::SceneImage, String> {
    let mut result = None;
    let mut request = Request {
        operation: IMAGE,
        input: bytes.as_ptr(),
        input_len: bytes.len(),
        output: (&mut result as *mut Option<Result<super::image::SceneImage, String>>).cast(),
    };
    #[cfg(target_arch = "wasm32")]
    {
        let address = (&mut request as *mut Request) as usize as u32;
        if LOADER.call(address).map_err(|error| error.to_string())? != 0 {
            return Err("unknown media operation".to_string());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // SAFETY: input and output pointers refer to this call's live borrows;
        // dispatch is synchronous and does not retain either pointer.
        if unsafe { dispatch_request(&mut request) } != 0 {
            return Err("unknown media operation".to_string());
        }
    }
    result.expect("media dispatch must write its result")
}

#[cfg(feature = "svg")]
pub(super) fn svg(source: &str) -> Result<Vec<crate::renderer::types::VelloPath>, String> {
    let mut result = None;
    let mut request = Request {
        operation: SVG,
        input: source.as_ptr(),
        input_len: source.len(),
        output: (&mut result
            as *mut Option<Result<Vec<crate::renderer::types::VelloPath>, String>>)
            .cast(),
    };
    #[cfg(target_arch = "wasm32")]
    {
        let address = (&mut request as *mut Request) as usize as u32;
        if LOADER.call(address).map_err(|error| error.to_string())? != 0 {
            return Err("unknown media operation".to_string());
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // SAFETY: input and output pointers refer to this call's live borrows;
        // dispatch is synchronous and does not retain either pointer.
        if unsafe { dispatch_request(&mut request) } != 0 {
            return Err("unknown media operation".to_string());
        }
    }
    result.expect("media dispatch must write its result")
}
