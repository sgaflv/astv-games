//! Asset generator for the gibbon game: writes `assets/gibbon.png`,
//! `assets/gibbon2.png` and `assets/guard.png`, 24x24 pixel-art sprite sheets
//! with five frames laid out horizontally:
//!
//! `[right0, right1, left0, left1, climb]`
//!
//! Run from the workspace root with `cargo run -p gibbon --example gen_assets`.
//! Regenerating is only needed when the art below changes.

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;

/// Frames per sheet and their pixel size.
const FRAME_W: usize = 24;
const FRAME_H: usize = 24;
const FRAMES: usize = 5;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Rgb(u8, u8, u8);

const TRANSPARENT: [u8; 4] = [0, 0, 0, 0];

fn rgba(c: Rgb) -> [u8; 4] {
    [c.0, c.1, c.2, 255]
}

/// A 24x24 RGBA canvas.
struct Canvas {
    pixels: Vec<[u8; 4]>,
}

impl Canvas {
    fn new() -> Canvas {
        Canvas {
            pixels: vec![TRANSPARENT; FRAME_W * FRAME_H],
        }
    }

    fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= FRAME_W as i32 || y >= FRAME_H as i32 {
            return;
        }
        self.pixels[y as usize * FRAME_W + x as usize] = rgba(c);
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for py in y..y + h {
            for px in x..x + w {
                self.set(px, py, c);
            }
        }
    }

    fn circle(&mut self, cx: i32, cy: i32, r: i32, c: Rgb) {
        for py in (cy - r)..=(cy + r) {
            for px in (cx - r)..=(cx + r) {
                let dx = px - cx;
                let dy = py - cy;
                if dx * dx + dy * dy <= r * r + r {
                    self.set(px, py, c);
                }
            }
        }
    }

    /// Mirror every pixel horizontally (for the left-facing frames).
    fn mirror(&mut self) {
        let mut out = vec![TRANSPARENT; FRAME_W * FRAME_H];
        for y in 0..FRAME_H {
            for x in 0..FRAME_W {
                out[y * FRAME_W + (FRAME_W - 1 - x)] = self.pixels[y * FRAME_W + x];
            }
        }
        self.pixels = out;
    }
}

struct Palette {
    body: Rgb,
    dark: Rgb,
    face: Rgb,
    eye: Rgb,
}

const GIBBON: Palette = Palette {
    body: Rgb(255, 168, 60),
    dark: Rgb(196, 116, 28),
    face: Rgb(255, 224, 172),
    eye: Rgb(20, 16, 28),
};

/// The green recolor of the player gibbon, used by player two.
const GIBBON2: Palette = Palette {
    body: Rgb(92, 188, 76),
    dark: Rgb(36, 118, 40),
    face: Rgb(176, 228, 132),
    eye: Rgb(20, 16, 28),
};

const GUARD: Palette = Palette {
    body: Rgb(210, 96, 128),
    dark: Rgb(148, 52, 84),
    face: Rgb(238, 150, 158),
    eye: Rgb(20, 16, 28),
};

/// Facing-right, standing pose.
fn draw_stand(c: &mut Canvas, p: &Palette) {
    // Tail hooking behind the body.
    c.set(6, 15, p.body);
    c.set(5, 14, p.body);
    c.set(5, 12, p.body);
    c.set(6, 11, p.body);
    c.set(7, 10, p.body);
    c.set(8, 9, p.body);

    // Back arm and leg (behind the torso, shaded).
    c.rect(7, 13, 2, 3, p.dark);
    c.rect(7, 19, 4, 3, p.dark);

    // Torso with a light belly.
    c.rect(8, 11, 11, 9, p.body);
    c.rect(10, 14, 6, 4, p.face);

    // Front arm hanging forward with a lighter hand.
    c.rect(16, 12, 3, 2, p.body);
    c.rect(18, 11, 2, 3, p.face);

    // Head with a face disc and eye.
    c.circle(14, 8, 5, p.face);
    c.rect(16, 6, 2, 2, p.eye);

    // Front leg and foot.
    c.rect(14, 19, 4, 3, p.body);
    c.rect(13, 21, 5, 2, p.dark);
}

/// Facing-right walking pose (legs and arms swapped relative to `stand`).
fn draw_walk(c: &mut Canvas, p: &Palette) {
    // Tail hooking behind the body.
    c.set(6, 15, p.body);
    c.set(5, 14, p.body);
    c.set(5, 12, p.body);
    c.set(6, 11, p.body);
    c.set(7, 10, p.body);
    c.set(8, 9, p.body);

    // Back arm swung back and leg bent up.
    c.rect(6, 13, 2, 2, p.dark);
    c.rect(7, 19, 4, 2, p.dark);

    // Torso.
    c.rect(8, 11, 11, 9, p.body);
    c.rect(10, 14, 6, 4, p.face);

    // Front arm swung forward.
    c.rect(17, 12, 3, 2, p.body);
    c.rect(19, 11, 2, 3, p.face);

    // Head.
    c.circle(14, 8, 5, p.face);
    c.rect(16, 6, 2, 2, p.eye);

    // Front leg stretched out.
    c.rect(15, 20, 4, 2, p.body);
    c.rect(13, 22, 6, 1, p.dark);
}

/// Symmetric climbing pose, facing the viewer on a ladder.
fn draw_climb(c: &mut Canvas, p: &Palette) {
    // Tail hooking below the body.
    c.set(7, 16, p.body);
    c.set(6, 17, p.body);
    c.set(6, 18, p.body);
    c.set(7, 19, p.body);

    // Arms reaching up.
    c.rect(6, 7, 3, 5, p.body);
    c.rect(15, 7, 3, 5, p.body);

    // Torso.
    c.rect(8, 11, 8, 9, p.body);
    c.rect(10, 14, 4, 4, p.face);

    // Head facing forward with two eyes.
    c.circle(12, 7, 5, p.face);
    c.rect(10, 6, 2, 2, p.eye);
    c.rect(14, 6, 2, 2, p.eye);

    // Legs spread on the rungs.
    c.rect(8, 20, 3, 3, p.body);
    c.rect(13, 20, 3, 3, p.body);
}

fn render_frame(frame: usize, p: &Palette) -> Canvas {
    let mut c = Canvas::new();
    match frame {
        0 => draw_stand(&mut c, p),
        1 => draw_walk(&mut c, p),
        2 => {
            draw_stand(&mut c, p);
            c.mirror();
        }
        3 => {
            draw_walk(&mut c, p);
            c.mirror();
        }
        _ => draw_climb(&mut c, p),
    }
    c
}

fn write_sheet(path: &PathBuf, p: &Palette) {
    let mut pixels: Vec<u8> = Vec::with_capacity(FRAME_W * FRAMES * FRAME_H * 4);
    for frame in 0..FRAMES {
        let canvas = render_frame(frame, p);
        for pixel in &canvas.pixels {
            pixels.extend_from_slice(pixel);
        }
    }

    let file = File::create(path).expect("create asset file");
    let mut encoder = png::Encoder::new(
        BufWriter::new(file),
        (FRAME_W * FRAMES) as u32,
        FRAME_H as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&pixels).expect("png image data");
    println!("wrote {}", path.display());
}

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let assets = manifest.join("assets");
    write_sheet(&assets.join("gibbon.png"), &GIBBON);
    write_sheet(&assets.join("gibbon2.png"), &GIBBON2);
    write_sheet(&assets.join("guard.png"), &GUARD);
}
