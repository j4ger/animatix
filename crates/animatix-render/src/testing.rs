//! Test-support helpers for GPU-dependent tests.
//!
//! The offscreen renderer needs a real adapter, which a bare CI container or a
//! headless dev box may not have. Tests call [`skip_if_no_gpu`] instead of
//! returning directly, so the decision is explicit and can be overridden:
//!
//! - locally (no `ANIMATIX_REQUIRE_GPU`): a missing adapter skips the test;
//! - in CI (`ANIMATIX_REQUIRE_GPU=1`): a missing adapter panics, because a
//!   green run that exercised no pixels is worse than a red one.

/// Fail when the environment demands a GPU and none is available.
///
/// Call this from the `Err(_)` arm of an adapter/device request, immediately
/// before returning early. See the module docs for the environment contract.
pub fn skip_if_no_gpu() {
    if std::env::var_os("ANIMATIX_REQUIRE_GPU").is_some() {
        panic!(
            "ANIMATIX_REQUIRE_GPU is set but no GPU adapter was available; \
             install a software Vulkan driver (mesa/lavapipe) or unset the variable"
        );
    }
}
