//! The virtual entry path, the embedded example library, and its content
//! digest.

/// Virtual path the single edited document is registered under. Imports of
/// other files resolve relative to it inside the in-memory source map: the
/// bundled library covers `../lib/*`, the shell supplies everything else via
/// `add_module` (learning the keys from `LoadResultDto::missing_imports`),
/// and anything still absent surfaces as a `FileNotFound` module error.
pub const ENTRY_PATH: &str = "main.amx";

/// The repo's shared example library (`examples/lib/*.amx`), embedded at
/// compile time and registered in the in-memory source map so example
/// imports (`import "../lib/theme.amx"`) resolve without any disk access.
///
/// Keys are the *resolved* import paths: `resolve_import` joins the import
/// string onto the importing file's directory and normalizes, which maps
/// every library reference onto the `../lib/<name>.amx` shape.
///
/// A `const` rather than a function body because [`identity_digest`] has to read
/// it at compile time: the embedded text is part of the artifact's fingerprint.
const BUNDLED_LIBRARY: &[(&str, &str)] = &[
    ("../lib/actions.amx", include_str!("../../../../examples/lib/actions.amx")),
    ("../lib/ambience.amx", include_str!("../../../../examples/lib/ambience.amx")),
    ("../lib/card.amx", include_str!("../../../../examples/lib/card.amx")),
    ("../lib/charts.amx", include_str!("../../../../examples/lib/charts.amx")),
    (
        "../lib/colorschemes.amx",
        include_str!("../../../../examples/lib/colorschemes.amx"),
    ),
    ("../lib/components.amx", include_str!("../../../../examples/lib/components.amx")),
    ("../lib/light.amx", include_str!("../../../../examples/lib/light.amx")),
    ("../lib/palette.amx", include_str!("../../../../examples/lib/palette.amx")),
    ("../lib/reexport.amx", include_str!("../../../../examples/lib/reexport.amx")),
    ("../lib/slide.amx", include_str!("../../../../examples/lib/slide.amx")),
    ("../lib/theme.amx", include_str!("../../../../examples/lib/theme.amx")),
    ("../lib/tokens.amx", include_str!("../../../../examples/lib/tokens.amx")),
    ("../lib/ui.amx", include_str!("../../../../examples/lib/ui.amx")),
];

pub fn bundled_library() -> &'static [(&'static str, &'static str)] {
    BUNDLED_LIBRARY
}

const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
const FNV_PRIME: u32 = 0x0100_0193;

/// FNV-1a over the version string and every embedded library file.
const fn fold(mut hash: u32, bytes: &[u8]) -> u32 {
    let mut i = 0;
    while i < bytes.len() {
        hash = hash.wrapping_mul(FNV_PRIME) ^ (bytes[i] as u32);
        i += 1;
    }
    hash
}

/// Length-prefix helper, so a rename that moves bytes from the key into the
/// source cannot produce the same digest.
const fn fold_len(mut hash: u32, len: u32) -> u32 {
    let mut shift = 0;
    while shift < 32 {
        hash = hash.wrapping_mul(FNV_PRIME) ^ ((len >> shift) & 0xff);
        shift += 8;
    }
    hash
}

/// The fingerprint of *this* engine bundle: its version and the exact text of
/// every `.amx` compiled into it.
///
/// This replaces a hand-incremented literal (`build_id() -> u32 { 54 }`) that
/// nothing verified. A number someone types by hand says nothing about what is
/// in the artifact, and the failure it was supposed to catch is precisely the one
/// it did not: a stale `web/pkg` served next to fresh scenes, which looked current
/// because the literal had been bumped in the same commit as the source change.
/// Here, editing any bundled `.amx` changes the digest, so an old bundle cannot
/// masquerade as a new one — and no `build.rs` is involved, which would make
/// `cargo test --workspace` depend on `.git` and make the artifact nondeterministic.
pub const fn identity_digest(version: &str, library: &[(&str, &str)]) -> u32 {
    let mut hash = fold(FNV_OFFSET_BASIS, version.as_bytes());
    let mut i = 0;
    while i < library.len() {
        let (key, source) = library[i];
        hash = fold_len(hash, key.len() as u32);
        hash = fold(hash, key.as_bytes());
        hash = fold_len(hash, source.len() as u32);
        hash = fold(hash, source.as_bytes());
        i += 1;
    }
    hash
}

/// The digest this build was compiled with.
pub const BUILD_ID: u32 = identity_digest(env!("CARGO_PKG_VERSION"), BUNDLED_LIBRARY);

#[cfg(test)]
mod tests {
    use super::*;

    /// The point of the digest: it is a function of the bundle's contents. With
    /// the old hand-typed literal this test could not exist, because nothing about
    /// `54` depended on anything.
    #[test]
    fn identity_digest_follows_version_and_contents() {
        // BUILD_ID must be *this* digest, not a number that happens to be stable.
        assert_eq!(BUILD_ID, identity_digest(env!("CARGO_PKG_VERSION"), BUNDLED_LIBRARY));
        // The embedded library is part of the input, not just the version.
        assert_ne!(BUILD_ID, identity_digest(env!("CARGO_PKG_VERSION"), &[]));
        // A version bump alone moves it.
        assert_ne!(
            identity_digest("0.1.0", BUNDLED_LIBRARY),
            identity_digest("0.2.0", BUNDLED_LIBRARY)
        );
        // One byte of scene text moves it.
        assert_ne!(
            identity_digest("0.1.0", &[("a", "x")]),
            identity_digest("0.1.0", &[("a", "y")])
        );
        // Length-prefixed, so bytes moving between key and source is not neutral.
        assert_ne!(
            identity_digest("0.1.0", &[("ab", "c")]),
            identity_digest("0.1.0", &[("a", "bc")])
        );
    }

    /// Two entries sharing a resolved key would make the second invisible in the
    /// browser: the source map is keyed by that path.
    #[test]
    fn bundled_library_keys_are_unique_and_resolved() {
        let mut seen = std::collections::HashSet::new();
        for (key, _) in bundled_library() {
            assert!(key.starts_with("../lib/"), "unexpected key shape: {key}");
            assert!(seen.insert(*key), "duplicate bundled library entry for {key}");
        }
    }
}
