pub mod engine;
pub mod game;

/// Local desktop window size. Independent of the 480x270 logical resolution;
/// on a 16:9 display it maps to an integer scale (960x540 -> x2).
const WINDOW_WIDTH: i32 = 960;
const WINDOW_HEIGHT: i32 = 540;

fn conf() -> miniquad::conf::Conf {
    miniquad::conf::Conf {
        window_title: "Snake".to_string(),
        window_width: WINDOW_WIDTH,
        window_height: WINDOW_HEIGHT,
        high_dpi: false,
        fullscreen: false,
        window_resizable: true,
        platform: miniquad::conf::Platform {
            swap_interval: Some(1),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Desktop entry point (x86_64-unknown-linux-gnu etc.).
#[cfg(not(target_os = "android"))]
pub fn desktop_main() {
    miniquad::start(conf(), || Box::new(engine::app::Stage::new()));
}

/// Android entry point, called from the Java glue (`quad_main`), which in turn
/// is invoked from `MainActivity.onCreate` after `System.loadLibrary("snake")`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    miniquad::start(conf(), || Box::new(engine::app::Stage::new()));
}
