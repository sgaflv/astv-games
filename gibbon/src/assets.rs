//! Compile-time asset registry: every file under this crate's `assets/` is
//! embedded into the binary by `build.rs` (via the shared
//! `engine/build_assets.rs` helper) and looked up by file name at runtime.
//! This works identically on desktop, Android and in tests with no filesystem
//! or APK packaging involved.

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Every embedded asset name, in no particular order.
pub fn names() -> impl Iterator<Item = &'static str> {
    ASSETS.iter().map(|(name, _)| *name)
}

/// Look up an embedded asset by its file name relative to `assets/`.
pub fn load(name: &str) -> Option<&'static [u8]> {
    ASSETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, data)| *data)
}
