//! Basic sprite support: a `Sprite` is a palette-indexed image (one byte per
//! pixel, converted from RGBA at load time) that can be blitted onto the
//! framebuffer; a `SpriteSheet` is a horizontal strip of equal-sized sprites
//! loaded from an embedded PNG asset.
//!
//! `RleSprite` stores the same palette indices as a compact run-length-encoded
//! stream (control bytes `xx yyyyyy`, see [`rle_encode`]) instead of one raw
//! byte per pixel, and [`RleSprite::draw`] walks the stream to blit only the
//! opaque runs. The stream is self-terminating, so the decoded size is the
//! bounding box of the opaque pixels rather than a fixed frame size.
//!
//! ```no_run
//! use snake::sprites::SpriteSheet;
//!
//! // 12 frames of 24x24 in assets/apple_rotate.png.
//! let sheet = SpriteSheet::load("apple_rotate.png", 24, 24, 12)?;
//! let apple = sheet.to_rle()?;
//! // apple[0].draw(&mut framebuffer, x, y);  // transparent pixels are skipped
//! # Ok::<(), snake::sprites::SpriteError>(())
//! ```

use std::error::Error;
use std::fmt;
use std::io::Cursor;

use crate::engine::assets;
pub use crate::engine::render::RleError;
use crate::engine::render::{
    CT_NEXT, CT_OPAQUE, CT_SKIP, MAX_RUN, Palette, Renderer, RleDecoder, TRANSPARENT,
};

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
    /// The embedded data is not a valid RLE sprite stream.
    Rle(RleError),
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
            SpriteError::Rle(err) => write!(f, "invalid RLE sprite data: {err}"),
        }
    }
}

impl Error for SpriteError {}

impl From<png::DecodingError> for SpriteError {
    fn from(err: png::DecodingError) -> SpriteError {
        SpriteError::Png(err)
    }
}

impl From<RleError> for SpriteError {
    fn from(err: RleError) -> SpriteError {
        SpriteError::Rle(err)
    }
}

/// A single decoded sprite as palette indices.
pub struct Sprite {
    width: usize,
    height: usize,
    /// Palette indices, one byte per pixel, row-major. Pixels holding
    /// `render::TRANSPARENT` are not drawn.
    indices: Vec<u8>,
}

impl Sprite {
    /// Wrap pre-converted palette indices (one byte per pixel, row-major).
    pub fn new(indices: Vec<u8>, width: usize, height: usize) -> Sprite {
        debug_assert_eq!(indices.len(), width * height);
        Sprite {
            width,
            height,
            indices,
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

    /// Raw palette indices, row-major, one byte per pixel.
    #[inline]
    pub fn pixels(&self) -> &[u8] {
        &self.indices
    }

    // /// Blit the sprite onto a renderer (e.g. the framebuffer) at `(x, y)`,
    // /// clipped to the screen. Transparent pixels are skipped.
    // pub fn draw(&self, r: &mut impl Renderer, x: i32, y: i32) {
    //     r.draw_image(x, y, &self.indices, self.width, self.height);
    // }
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

        // Preprocess each frame into palette indices once, at load time, so
        // drawing is a pure blit (see `Palette::quantize_rgba`).
        let palette = Palette::default();
        let mut frames = Vec::with_capacity(sprite_count);
        for frame in 0..sprite_count {
            let origin = frame * size_x;
            let mut indices = vec![0; size_x * size_y];
            for row in 0..size_y {
                let src = (row * image.width + origin) * CHANNELS;
                let dst = row * size_x;
                for col in 0..size_x {
                    let o = src + col * CHANNELS;
                    let rgba = [
                        image.pixels[o],
                        image.pixels[o + 1],
                        image.pixels[o + 2],
                        image.pixels[o + 3],
                    ];
                    indices[dst + col] = palette.quantize_rgba(rgba);
                }
            }
            frames.push(Sprite::new(indices, size_x, size_y));
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

    /// Encode every frame as an [`RleSprite`], so the game can draw them with
    /// [`RleSprite::draw`] (which blits only opaque runs through
    /// `Renderer::draw_rle_image`) instead of the raw-index [`Sprite`] blit.
    /// The conversion runs once at load time; each encoded frame keeps its
    /// opaque-pixel bounding box.
    pub fn to_rle(&self) -> Result<Vec<RleSprite>, RleError> {
        self.frames.iter().map(RleSprite::from_sprite).collect()
    }
}

/// Encode palette-indexed pixels (row-major, one byte per pixel) into the RLE
/// stream decoded by [`RleSprite`]. `width` is the row stride and is only used
/// to know where rows wrap; it is not stored in the output.
///
/// The stream is a sequence of control bytes of the form `xx yyyyyy`: the two
/// type bits `xx` select one of three commands and the six low bits carry a
/// count `n` from 0 to 63.
///
/// * type `00` is followed by `n` opaque palette indices on the current scan
///   line;
/// * type `01` skips `n` pixels as transparent;
/// * type `10` skips `n` scan lines and resumes at the first pixel of that
///   line, so count 1 switches to the next scan line.
///
/// The sprite ends when the byte array does, so there is no size field. Runs
/// longer than 63 are split into several control bytes; fully transparent
/// scan lines are folded into a single `10` skip and trailing transparent
/// pixels are omitted (the line's terminating `10` skip implies them), so the
/// decoded size is the bounding box of the opaque pixels.
pub fn rle_encode(indices: &[u8], width: usize) -> Vec<u8> {
    let mut out = Vec::new();
    if width == 0 {
        return out;
    }
    let height = indices.len() / width;
    let mut row = 0usize; // last scan line already emitted

    for y in 0..height {
        let o = y * width;
        if indices[o..o + width].iter().all(|&p| p == TRANSPARENT) {
            continue;
        }
        // Jump to this line, folding any empty lines in between into one skip.
        let mut skip = y - row;
        while skip >= MAX_RUN {
            out.push(CT_NEXT | MAX_RUN as u8);
            skip -= MAX_RUN;
        }
        if skip > 0 {
            out.push(CT_NEXT | skip as u8);
        }
        row = y;

        let mut x = 0;
        while x < width {
            if indices[o + x] == TRANSPARENT {
                let mut end = x;
                while end < width && indices[o + end] == TRANSPARENT {
                    end += 1;
                }
                if end == width {
                    break; // trailing transparent pixels: implied by the `10` skip
                }
                let mut n = end - x;
                while n >= MAX_RUN {
                    out.push(CT_SKIP | MAX_RUN as u8);
                    n -= MAX_RUN;
                }
                if n > 0 {
                    out.push(CT_SKIP | n as u8);
                }
                x = end;
            } else {
                let mut end = x;
                while end < width && indices[o + end] != TRANSPARENT {
                    end += 1;
                }
                let mut n = end - x;
                while n >= MAX_RUN {
                    out.push(CT_OPAQUE | MAX_RUN as u8);
                    out.extend_from_slice(&indices[o + x..o + x + MAX_RUN]);
                    x += MAX_RUN;
                    n -= MAX_RUN;
                }
                if n > 0 {
                    out.push(CT_OPAQUE | n as u8);
                    out.extend_from_slice(&indices[o + x..o + x + n]);
                    x += n;
                }
            }
        }
    }
    out
}

/// A sprite whose pixels are stored as an RLE stream (see [`rle_encode`])
/// instead of one raw byte per pixel. The stream is self-terminating, so
/// `width`/`height` are the bounding box of the opaque pixels rather than a
/// fixed frame size; drawing walks the stream and blits each opaque run, which
/// never touches the transparent pixels at all.
#[derive(Clone)]
pub struct RleSprite {
    data: Vec<u8>,
    width: usize,
    height: usize,
}

impl RleSprite {
    /// Parse and validate an RLE stream, computing the opaque-pixel bounding
    /// box. Rejects undefined control types (`11`) and opaque runs that run
    /// past the end of the data.
    pub fn new(data: Vec<u8>) -> Result<RleSprite, RleError> {
        let mut decoder = RleDecoder::new(&data);
        let mut width = 0;
        let mut height = 0;
        while let Some(run) = decoder.next_run()? {
            width = width.max(run.x + run.indices.len());
            height = height.max(run.y + 1);
        }
        Ok(RleSprite {
            data,
            width,
            height,
        })
    }

    /// Load a pre-encoded RLE sprite from the embedded `assets/` directory.
    pub fn load(name: &str) -> Result<RleSprite, SpriteError> {
        let data =
            assets::load(name).ok_or_else(|| SpriteError::AssetNotFound(name.to_string()))?;
        RleSprite::new(data.to_vec()).map_err(SpriteError::Rle)
    }

    /// Encode a palette-indexed sprite into RLE.
    pub fn from_sprite(sprite: &Sprite) -> Result<RleSprite, RleError> {
        RleSprite::new(rle_encode(sprite.pixels(), sprite.width()))
    }

    /// Width of the opaque-pixel bounding box inferred from the stream.
    #[inline]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Height of the opaque-pixel bounding box inferred from the stream.
    #[inline]
    pub fn height(&self) -> usize {
        self.height
    }

    /// The raw RLE stream.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Blit the sprite onto a renderer (e.g. the framebuffer) at `(x, y)`,
    /// clipped to the screen. Only opaque runs are touched, so transparent
    /// pixels never write to the target.
    pub fn draw(&self, r: &mut impl Renderer, x: i32, y: i32) {
        r.draw_rle_image(x, y, &self.data);
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
    use crate::engine::render::{CT_COUNT, Color, Framebuffer, TRANSPARENT};

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
            assert_eq!(sprite.pixels().len(), 24 * 24);
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
        let palette = Palette::default();
        assert_eq!(sheet.len(), 2);
        assert_eq!(
            sheet.sprite(0).unwrap().pixels(),
            &[palette.index_of(Color::rgb(255, 0, 0))]
        );
        assert_eq!(
            sheet.sprite(1).unwrap().pixels(),
            &[palette.index_of(Color::rgb(0, 0, 255))]
        );
    }

    #[test]
    fn loading_thresholds_alpha_into_transparency() {
        // 2x1 strip: opaque red, and a blue pixel that is 50% transparent.
        let mut strip = Vec::new();
        strip.extend_from_slice(&[255, 0, 0, 255]);
        strip.extend_from_slice(&[0, 0, 255, 128]);
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

        let sheet = SpriteSheet::from_png(&png_bytes, 2, 1, 1).expect("2x1 sheet loads");
        let palette = Palette::default();
        let indices = sheet.sprite(0).unwrap().pixels();
        assert_eq!(indices[0], palette.index_of(Color::rgb(255, 0, 0)));
        assert_eq!(indices[1], TRANSPARENT);
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
    fn rle_encode_exact_stream() {
        // Row 0: A . B, row 1 empty, row 2: C . .  (3x3 frame).
        let indices = [
            1,
            TRANSPARENT,
            2,
            TRANSPARENT,
            TRANSPARENT,
            TRANSPARENT,
            3,
            TRANSPARENT,
            TRANSPARENT,
        ];
        let out = rle_encode(&indices, 3);
        assert_eq!(out, vec![0x01, 1, 0x41, 0x01, 2, 0x82, 0x01, 3]);
        let sprite = RleSprite::new(out).unwrap();
        assert_eq!(sprite.width(), 3);
        assert_eq!(sprite.height(), 3);
    }

    #[test]
    fn rle_hand_written_stream_decodes_size_and_pixels() {
        // Two single-pixel runs at (2,0) and (3,0).
        let stream = vec![0x42, 0x01, 3, 0x01, 4];
        let sprite = RleSprite::new(stream).unwrap();
        assert_eq!(sprite.width(), 4);
        assert_eq!(sprite.height(), 1);

        let mut fb = Framebuffer::new();
        fb.clear(Color::BLACK);
        sprite.draw(&mut fb, 0, 0);
        assert_eq!(fb.pixels()[0], 0);
        assert_eq!(fb.pixels()[1], 0);
        assert_eq!(fb.pixels()[2], 3);
        assert_eq!(fb.pixels()[3], 4);
    }

    #[test]
    fn rle_long_runs_split_into_max_count() {
        let width = 100;
        let indices = vec![7u8; width];
        let out = rle_encode(&indices, width);
        assert_eq!(out.len(), 2 + width);
        assert_eq!(out[0] >> 6, 0);
        assert_eq!(out[0] & CT_COUNT, MAX_RUN as u8);
        assert_eq!(out[64] >> 6, 0);
        assert_eq!(out[64] & CT_COUNT, (width - MAX_RUN) as u8);
        let sprite = RleSprite::new(out).unwrap();
        assert_eq!(sprite.width(), 100);
        assert_eq!(sprite.height(), 1);
    }

    #[test]
    fn rle_empty_rows_are_merged_into_one_skip() {
        // Row 0 content, rows 1-2 empty, row 3 content.
        let width = 2;
        let indices = [
            5,
            6,
            TRANSPARENT,
            TRANSPARENT,
            TRANSPARENT,
            TRANSPARENT,
            8,
            9,
        ];
        let out = rle_encode(&indices, width);
        assert_eq!(out, vec![0x02, 5, 6, 0x83, 0x02, 8, 9]);
        let sprite = RleSprite::new(out).unwrap();
        assert_eq!(sprite.width(), 2);
        assert_eq!(sprite.height(), 4);
    }

    #[test]
    fn rle_trailing_transparent_pixels_are_omitted() {
        let indices = [9, TRANSPARENT, TRANSPARENT];
        let out = rle_encode(&indices, 3);
        assert_eq!(out, vec![0x01, 9]);
        let sprite = RleSprite::new(out).unwrap();
        assert_eq!(sprite.width(), 1);
        assert_eq!(sprite.height(), 1);
    }

    #[test]
    fn rle_all_transparent_encodes_empty() {
        let out = rle_encode(&[TRANSPARENT; 12], 3);
        assert!(out.is_empty());
        let sprite = RleSprite::new(out).unwrap();
        assert_eq!(sprite.width(), 0);
        assert_eq!(sprite.height(), 0);
    }

    #[test]
    fn rle_malformed_streams_are_rejected() {
        // Opaque run claims 5 bytes but only 2 remain.
        let truncated = vec![0x05, 1, 2];
        assert!(matches!(
            RleSprite::new(truncated),
            Err(RleError::TruncatedRun { .. })
        ));
        // Undefined control type 11.
        let reserved = vec![0xC0, 1];
        assert!(matches!(
            RleSprite::new(reserved),
            Err(RleError::ReservedControlType { .. })
        ));
    }
}
