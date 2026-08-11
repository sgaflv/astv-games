#[cfg(not(target_os = "android"))]
fn main() {
    snake::desktop_main();
}
