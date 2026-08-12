//! Basic sprite support: a `Sprite` is a decoded RGBA8 image that can be drawn
//! onto the framebuffer; a `SpriteSheet` is a horizontal strip of equal-sized
//! sprites loaded from an embedded PNG asset.
//!
//! ```no_run
//! use snake::sprites::SpriteSheet;
//!
//! // 12 frames of 24x24 in assets/apple_rotate.png.
//! let sheet = SpriteSheet::load("apple_rotate.png", 24, 24, 12)?;
//! let apple = sheet.sprite(0).unwrap();
//! // apple.draw(&mut framebuffer, x, y);  // alpha-composited blit
//! # Ok::<(), snake::sprites::SpriteError>(())
//! ```

use std::error::Error;
use std::fmt;
use std::io::Cursor;

use crate::assets;
use crate::engine::render::Renderer;

/// Bytes per pixel in a decoded sprite.
const CHANNELS: usize = 4;

/// Errors raised while loading a sprite sheet.
#[derive(Debug)]
pub enum SpriteError {
    /// The requested file is not in the embedded `assets/` registry.
    AssetNotFound(String),
    /// The embedded data could not be decoded as a PNG.
    Png(png::DecodingError),
    /// PNG data that is not 8-bit RGBA/RGB after transformation.
    UnsupportedFormat,
    /// The strip does not match `size_x * sprite_count` wide / `size_y` tall.
    DimensionsMismatch { width: usize, height: usize },
    /// `sprite_count` must be non-zero.
    NoSprites,
}

impl fmt::Display for SpriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SpriteError::AssetNotFound(name) => {
                write!(f, "asset not found in embedded assets/: {name}")
            }
            SpriteError::Png(err) => write!(f, "failed to decode PNG: {err}"),
            SpriteError::UnsupportedFormat => {
                write!(f, "unsupported PNG format (expected 8-bit RGBA)")
            }
            SpriteError::DimensionsMismatch { width, height } => write!(
                f,
                "sprite strip is {width}x{height}; it must be exactly size_x * sprite_count wide and size_y tall"
            ),
            SpriteError::NoSprites => write!(f, "sprite_count must be at least 1"),
        }
    }
}

impl Error for SpriteError {}

impl From<png::DecodingError> for SpriteError {
    fn from(err: png::DecodingError) -> SpriteError {
        SpriteError::Png(err)
    }
}

/// A single decoded RGBA8 image.
pub struct Sprite {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl Sprite {
    /// Wrap pre-decoded RGBA8 pixels (4 bytes per pixel, row-major).
    pub fn new(pixels: Vec<u8>, width: usize, height: usize) -> Sprite {
        debug_assert_eq!(pixels.len(), width * height * CHANNELS);
        Sprite {
            width,
            height,
            pixels,
        }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Raw RGBA8 pixels, row-major.
    #[inline]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Draw the sprite onto a renderer (e.g. the framebuffer) at `(x, y)`,
    /// alpha-composited and clipped.
    pub fn draw(&self, r: &mut impl Renderer, x: i32, y: i32) {
        r.draw_image(x, y, &self.pixels, self.width, self.height);
    }
}

/// A horizontal strip of equal-sized sprites decoded from one PNG.
pub struct SpriteSheet {
    frames: Vec<Sprite>,
    frame_width: usize,
    frame_height: usize,
}

impl SpriteSheet {
    /// Load a sprite sheet from the embedded `assets/` directory.
    ///
    /// `name` is the file name relative to `assets/`, `size_x`/`size_y` the
    /// size of one frame and `sprite_count` the number of frames laid out side
    /// by side in the image.
    pub fn load(
        name: &str,
        size_x: usize,
        size_y: usize,
        sprite_count: usize,
    ) -> Result<SpriteSheet, SpriteError> {
        let data =
            assets::load(name).ok_or_else(|| SpriteError::AssetNotFound(name.to_string()))?;
        SpriteSheet::from_png(data, size_x, size_y, sprite_count)
    }

    /// Decode a sprite sheet from raw PNG bytes.
    pub fn from_png(
        data: &[u8],
        size_x: usize,
        size_y: usize,
        sprite_count: usize,
    ) -> Result<SpriteSheet, SpriteError> {
        if sprite_count == 0 {
            return Err(SpriteError::NoSprites);
        }
        let image = decode_png(data)?;
        if image.width != size_x * sprite_count || image.height != size_y {
            return Err(SpriteError::DimensionsMismatch {
                width: image.width,
                height: image.height,
            });
        }

        let mut frames = Vec::with_capacity(sprite_count);
        for frame in 0..sprite_count {
            let origin = frame * size_x;
            let mut pixels = vec![0; size_x * size_y * CHANNELS];
            for row in 0..size_y {
                let src = (row * image.width + origin) * CHANNELS;
                let dst = row * size_x * CHANNELS;
                pixels[dst..dst + size_x * CHANNELS]
                    .copy_from_slice(&image.pixels[src..src + size_x * CHANNELS]);
            }
            frames.push(Sprite::new(pixels, size_x, size_y));
        }

        Ok(SpriteSheet {
            frames,
            frame_width: size_x,
            frame_height: size_y,
        })
    }

    /// One sprite frame. Out-of-range indices return `None`.
    pub fn sprite(&self, index: usize) -> Option<&Sprite> {
        self.frames.get(index)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Size of a single frame: `(width, height)`.
    pub fn frame_size(&self) -> (usize, usize) {
        (self.frame_width, self.frame_height)
    }
}

/// A fully decoded RGBA8 PNG.
struct RgbaImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

/// Decode a PNG into RGBA8, expanding RGB to RGBA and stripping 16-bit data.
fn decode_png(data: &[u8]) -> Result<RgbaImage, SpriteError> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::ALPHA | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let width = reader.info().width as usize;
    let height = reader.info().height as usize;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or(SpriteError::UnsupportedFormat)?;
    let mut buffer = vec![0; buffer_size];
    reader.next_frame(&mut buffer)?;
    reader.finish()?;

    let (color_type, bit_depth) = reader.output_color_type();
    let (pixels, channels) = match (color_type, bit_depth) {
        (png::ColorType::Rgba, png::BitDepth::Eight) => (buffer, CHANNELS),
        (png::ColorType::Rgb, png::BitDepth::Eight) => {
            let mut rgba = Vec::with_capacity(width * height * CHANNELS);
            for rgb in buffer.chunks_exact(3) {
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
            (rgba, CHANNELS)
        }
        _ => return Err(SpriteError::UnsupportedFormat),
    };
    debug_assert_eq!(pixels.len(), width * height * channels);

    Ok(RgbaImage {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::render::{Color, Framebuffer};

    fn apple_sheet() -> SpriteSheet {
        SpriteSheet::load("apple_rotate.png", 24, 24, 12).expect("apple sheet loads")
    }

    #[test]
    fn apple_sheet_has_twelve_frames() {
        let sheet = apple_sheet();
        assert_eq!(sheet.len(), 12);
        assert_eq!(sheet.frame_size(), (24, 24));
        for frame in 0..sheet.len() {
            let sprite = sheet.sprite(frame).expect("frame exists");
            assert_eq!(sprite.width(), 24);
            assert_eq!(sprite.height(), 24);
            assert_eq!(sprite.pixels().len(), 24 * 24 * 4);
        }
    }

    #[test]
    fn frames_are_cropped_from_the_strip() {
        // Synthetic 2-frame strip: frame 0 all red, frame 1 all blue.
        let mut strip = Vec::new();
        for pixel in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            strip.extend_from_slice(&pixel);
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("png header");
            writer
                .write_image_data(&strip)
                .expect("png write image data");
        }

        let sheet = SpriteSheet::from_png(&png_bytes, 1, 1, 2).expect("2x1 sheet loads");
        assert_eq!(sheet.len(), 2);
        assert_eq!(sheet.sprite(0).unwrap().pixels(), &[255, 0, 0, 255]);
        assert_eq!(sheet.sprite(1).unwrap().pixels(), &[0, 0, 255, 255]);
    }

    #[test]
    fn bad_dimensions_are_rejected() {
        let data = assets::load("apple_rotate.png").unwrap();
        // Sheet is 288 wide; 12 frames of 24 expect exactly 288.
        assert!(SpriteSheet::from_png(data, 24, 24, 12).is_ok());
        assert!(SpriteSheet::from_png(data, 32, 24, 12).is_err());
        assert!(SpriteSheet::from_png(data, 24, 32, 12).is_err());
        assert!(SpriteSheet::from_png(data, 24, 24, 0).is_err());
    }

    #[test]
    fn unknown_asset_is_an_error() {
        assert!(matches!(
            SpriteSheet::load("missing.png", 8, 8, 1),
            Err(SpriteError::AssetNotFound(_))
        ));
    }

    #[test]
    fn sprite_draws_onto_the_framebuffer() {
        let sheet = apple_sheet();
        let apple = sheet.sprite(0).unwrap();
        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        apple.draw(&mut fb, 10, 10);
        // The apple is opaque somewhere in its frame; the top-left corner of
        // the drawn region must differ from the background where the sprite
        // covers it. Check the exact corner is transparent-safe (never panics)
        // and that at least one pixel in the frame changed.
        let changed = fb.pixels().chunks_exact(3).any(|p| p != [0, 0, 0]);
        assert!(changed, "sprite must paint non-background pixels");
    }
}
