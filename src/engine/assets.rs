//! Compile-time asset registry: every file under `assets/` is embedded into
//! the binary by `build.rs` and looked up by file name at runtime. This works
//! identically on desktop, Android and in tests with no filesystem or APK
//! packaging involved.

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// Look up an embedded asset by its file name relative to `assets/`.
pub fn load(name: &str) -> Option<&'static [u8]> {
    ASSETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, data)| *data)
}

/// All embedded asset file names, sorted as written by `build.rs`.
pub fn names() -> impl Iterator<Item = &'static str> {
    ASSETS.iter().map(|(n, _)| *n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_apple_is_a_png() {
        let data = load("apple_rotate.png").expect("apple_rotate.png is embedded");
        // PNG signature: 89 50 4E 47 0D 0A 1A 0A.
        assert_eq!(&data[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn unknown_asset_is_not_found() {
        assert!(load("does_not_exist.png").is_none());
    }
}
