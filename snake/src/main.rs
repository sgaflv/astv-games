fn main() {
    #[cfg(not(target_os = "android"))]
    snake::desktop_main();
}
