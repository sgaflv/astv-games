use snake::engine::render::{Framebuffer, Renderer, WIDTH};

fn dump(fb: &Framebuffer, label: &str, x0: i32, y0: i32) {
    println!("== {label} ==");
    for y in y0..y0 + 8 {
        let mut line = String::new();
        for x in x0..x0 + 8 {
            // One palette index per pixel; anything but index 0 is painted.
            let i = (y as usize) * WIDTH + x as usize;
            line.push(if fb.pixels()[i] != 0 { '#' } else { '.' });
        }
        println!("{line}");
    }
}

fn main() {
    let mut fb = Framebuffer::new();
    fb.clear(snake::engine::color::Color::BLACK);
    // Draw each glyph starting on a fresh 8x8 block, spaced 2px apart.
    for (i, ch) in ['F', '9', 'b', 'd', '6', 'p'].iter().enumerate() {
        let x = i as i32 * 10;
        fb.draw_text(x, 0, 1, snake::engine::color::Color::WHITE, &ch.to_string());
    }
    dump(&fb, "F", 0, 0);
    dump(&fb, "9", 10, 0);
    dump(&fb, "b", 20, 0);
    dump(&fb, "d", 30, 0);
    dump(&fb, "6", 40, 0);
    dump(&fb, "p", 50, 0);
}
