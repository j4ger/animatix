//! Virtual document entry and compact diagnostic identity.

/// Imports are relative to this source-map key. The host must supply every
/// imported module; scene libraries are content, not implicit engine built-ins.
pub const ENTRY_PATH: &str = "main.amx";

const fn diagnostic_id(identity: &str) -> u32 {
    let bytes = identity.as_bytes();
    let mut hash = 0x811c_9dc5u32;
    let mut i = 0;
    while i < bytes.len() {
        hash = hash.wrapping_mul(0x0100_0193) ^ bytes[i] as u32;
        i += 1;
    }
    hash
}

/// A compact diagnostic value, not an integrity check. The SDK loader verifies
/// the full runtime_build_id and manifest file hashes.
pub const BUILD_ID: u32 = diagnostic_id(match option_env!("ANIMATIX_RUNTIME_BUILD_ID") {
    Some(identity) => identity,
    None => env!("CARGO_PKG_VERSION"),
});

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostic_identity_changes_with_sdk_identity() {
        assert_ne!(super::diagnostic_id("build-a"), super::diagnostic_id("build-b"));
    }
}
