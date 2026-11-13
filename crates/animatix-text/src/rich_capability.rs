//! Typst compilation is a cold, synchronously callable Wasm capability.

use crate::{CachedGroupedText, CompiledText, FontContext, TextCompileRequest};
use animatix_core::error::RenderError;

#[cfg(feature = "rich-text")]
mod implementation;

#[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
enum Operation<'a> {
    Compile(&'a TextCompileRequest<'a>, &'a FontContext),
    Grouped(&'a TextCompileRequest<'a>, &'a FontContext),
}
#[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
enum Output {
    Compile(Result<CompiledText, RenderError>),
    Grouped(Result<CachedGroupedText, RenderError>),
}
#[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
struct Dispatch<'a> {
    operation: Operation<'a>,
    output: Option<Output>,
}

// A scalar ABI keeps the split boundary explicit. The pointed-to request is
// borrowed exclusively for this synchronous call and is never retained.
#[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
fn dispatch(address: u32) -> u32 {
    // SAFETY: `invoke` passes a valid, exclusively borrowed Dispatch living
    // until this function returns; both modules share the same Wasm memory.
    let request = unsafe { &mut *(address as *mut Dispatch<'_>) };
    request.output = Some(match &request.operation {
        Operation::Compile(text, fonts) => Output::Compile(implementation::compile(text, fonts)),
        Operation::Grouped(text, fonts) => Output::Grouped(implementation::grouped(text, fonts)),
    });
    0
}

#[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
static LOADER: wasm_split::LazyLoader<u32, u32> =
    wasm_split::lazy_loader!(extern "rich" fn dispatch(address: u32) -> u32);

/// Load the rich compiler before building scenes; never suspend frame evaluation.
#[cfg(target_arch = "wasm32")]
pub async fn activate_rich_capability() -> Result<(), String> {
    #[cfg(feature = "rich-text")]
    {
        if !LOADER.load().await {
            return Err("rich text capability could not load".to_string());
        }
        Ok(())
    }
    #[cfg(not(feature = "rich-text"))]
    {
        Err("rich text capability is not compiled in".to_string())
    }
}

#[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
fn invoke(operation: Operation<'_>) -> Result<Output, RenderError> {
    let mut request = Dispatch {
        operation,
        output: None,
    };
    let address = (&mut request as *mut Dispatch<'_>) as u32;
    LOADER
        .call(address)
        .map_err(|error| RenderError::TextCompilation(error.to_string()))?;
    request.output.ok_or_else(|| {
        RenderError::TextCompilation("rich text capability returned no result".to_string())
    })
}

pub(crate) fn compile(
    request: &TextCompileRequest<'_>,
    fonts: &FontContext,
) -> Result<CompiledText, RenderError> {
    #[cfg(all(feature = "rich-text", not(target_arch = "wasm32")))]
    {
        implementation::compile(request, fonts)
    }
    #[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
    {
        match invoke(Operation::Compile(request, fonts))? {
            Output::Compile(result) => result,
            Output::Grouped(_) => unreachable!("rich dispatch returned the wrong result"),
        }
    }
    #[cfg(not(feature = "rich-text"))]
    {
        let _ = fonts;
        Err(RenderError::TextCompilation(format!(
            "{:?} requires the rich-text capability",
            request.kind
        )))
    }
}

pub(crate) fn grouped(
    request: &TextCompileRequest<'_>,
    fonts: &FontContext,
) -> Result<CachedGroupedText, RenderError> {
    #[cfg(all(feature = "rich-text", not(target_arch = "wasm32")))]
    {
        implementation::grouped(request, fonts)
    }
    #[cfg(all(feature = "rich-text", target_arch = "wasm32"))]
    {
        match invoke(Operation::Grouped(request, fonts))? {
            Output::Grouped(result) => result,
            Output::Compile(_) => unreachable!("rich dispatch returned the wrong result"),
        }
    }
    #[cfg(not(feature = "rich-text"))]
    {
        let _ = (request, fonts);
        Err(RenderError::TextCompilation(
            "Equation requires the rich-text capability".to_string(),
        ))
    }
}
