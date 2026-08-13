fn main() {
    #[cfg(not(target_os = "android"))]
    app::desktop_main();
}
