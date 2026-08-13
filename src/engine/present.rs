use crate::engine::{color, render};

use miniquad::{
    Bindings, BufferId, BufferLayout, BufferSource, BufferType, BufferUsage, FilterMode,
    MipmapFilterMode, Pipeline, PipelineParams, RenderingBackend, ShaderMeta, ShaderSource,
    TextureAccess, TextureFormat, TextureId, TextureKind, TextureParams, TextureSource,
    TextureWrap, UniformBlockLayout, VertexAttribute, VertexFormat,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    x: f32,
    y: f32,
    u: f32,
    v: f32,
}

const VERT: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec2 in_uv;

varying lowp vec2 v_uv;

void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    v_uv = in_uv;
}
"#;

const FRAG: &str = r#"#version 100
precision mediump float;

varying lowp vec2 v_uv;
uniform sampler2D tex;
uniform sampler2D palette;

void main() {
    // tex.r holds the 8-bit palette index (an R8 texture, or the index
    // replicated across RGB on the GLES2 fallback).
    lowp float index = texture2D(tex, v_uv).r * 255.0;
    // Sample the 256x1 palette at the center of entry `index`.
    lowp vec2 p_uv = vec2((index + 0.5) / 256.0, 0.5);
    gl_FragColor = texture2D(palette, p_uv);
}
"#;

fn shader_meta() -> ShaderMeta {
    ShaderMeta {
        images: vec!["tex".to_string(), "palette".to_string()],
        uniforms: UniformBlockLayout { uniforms: vec![] },
    }
}

/// True when an 8-bit red (R8) texture upload is available. miniquad's `Alpha`
/// format maps to R8/GL_RED on native and GL_ALPHA on WASM; GL_R8 needs
/// OpenGL 3.0+, GLES3 or WebGL2. GLES2 (the Android context) and WebGL1 cannot
/// do R8, so the presenter replicates the index across an RGB8 texture instead
/// and the shader reads `.r`, which samples identically.
fn r8_texture_supported(ctx: &dyn RenderingBackend) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        // miniquad always uses GL_ALPHA for `Alpha` on WASM, which samples
        // zero in the red channel; replicate on RGB8 there.
        let _ = ctx;
        return false;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        let version = ctx.info().gl_version_string;
        !version.starts_with("OpenGL ES 2") && version != "WebGL 1.0"
    }
}

/// Presents the 480x270 indexed CPU framebuffer (one palette index per pixel)
/// to the physical screen.
///
/// GPU work per frame is exactly one index texture upload plus one fullscreen
/// quad. The palette lives in a separate 256x1 texture; the fragment shader
/// looks up each index in it, so the CPU never expands indices to RGB. The
/// quad is drawn with nearest-neighbour filtering at an integer scale factor
/// and the window is cleared with the background color so any letterbox bars
/// blend into the frame.
pub struct Presenter {
    ctx: Box<dyn RenderingBackend>,
    index_texture: TextureId,
    pipeline: Pipeline,
    quad: BufferId,
    bindings: Bindings,
    // Physical window size in pixels.
    window_w: i32,
    window_h: i32,
    // Integer upscale factor currently in effect.
    scale: i32,
    /// Bytes per index pixel uploaded to the GPU: 1 (R8) or 3 (RGB8
    /// replication on GLES2/WebGL1).
    index_bpp: u32,
    /// Staging buffer for the RGB8-replicated upload path.
    rgb_scratch: Vec<u8>,
}

impl Default for Presenter {
    fn default() -> Presenter {
        Presenter::new()
    }
}

impl Presenter {
    pub fn new() -> Presenter {
        let mut ctx: Box<dyn RenderingBackend> = miniquad::window::new_rendering_backend();

        let index_bpp: u32 = if r8_texture_supported(&*ctx) { 1 } else { 3 };
        let index_format = if index_bpp == 1 {
            TextureFormat::Alpha
        } else {
            TextureFormat::RGB8
        };

        let index_texture = ctx.new_texture(
            TextureAccess::Static,
            TextureSource::Empty,
            TextureParams {
                kind: TextureKind::Texture2D,
                width: render::WIDTH as u32,
                height: render::HEIGHT as u32,
                format: index_format,
                wrap: TextureWrap::Clamp,
                min_filter: FilterMode::Nearest,
                mag_filter: FilterMode::Nearest,
                mipmap_filter: MipmapFilterMode::None,
                allocate_mipmaps: false,
                sample_count: 1,
            },
        );

        // 256x1 RGB palette texture, indexed by the render buffer pixels.
        let palette_texture = ctx.new_texture(
            TextureAccess::Static,
            TextureSource::Empty,
            TextureParams {
                kind: TextureKind::Texture2D,
                width: color::PALETTE_SIZE as u32,
                height: 1,
                format: TextureFormat::RGB8,
                wrap: TextureWrap::Clamp,
                min_filter: FilterMode::Nearest,
                mag_filter: FilterMode::Nearest,
                mipmap_filter: MipmapFilterMode::None,
                allocate_mipmaps: false,
                sample_count: 1,
            },
        );
        // The palette is fixed at construction and matches the framebuffer's
        // default palette, so this upload happens once.
        ctx.texture_update(palette_texture, color::Palette::default().bytes());

        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: VERT,
                    fragment: FRAG,
                },
                shader_meta(),
            )
            .expect("failed to compile presentation shader");

        let pipeline = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
            ],
            shader,
            PipelineParams::default(),
        );

        let quad = ctx.new_buffer(
            BufferType::VertexBuffer,
            BufferUsage::Dynamic,
            BufferSource::empty::<Vertex>(4 * std::mem::size_of::<Vertex>()),
        );
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let index = ctx.new_buffer(
            BufferType::IndexBuffer,
            BufferUsage::Immutable,
            BufferSource::slice(&indices),
        );

        let bindings = Bindings {
            vertex_buffers: vec![quad],
            index_buffer: index,
            images: vec![index_texture, palette_texture],
        };

        let (window_w, window_h) = miniquad::window::screen_size();
        let mut presenter = Presenter {
            ctx,
            index_texture,
            pipeline,
            quad,
            bindings,
            window_w: window_w as i32,
            window_h: window_h as i32,
            scale: 1,
            index_bpp,
            rgb_scratch: vec![0; render::WIDTH * render::HEIGHT * 3],
        };
        presenter.update_viewport(window_w as i32, window_h as i32);
        presenter
    }

    /// Recompute the integer upscale and centered viewport for a new window
    /// size and rewrite the quad vertices in normalized device coordinates.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.update_viewport(width as i32, height as i32);
    }

    fn update_viewport(&mut self, w: i32, h: i32) {
        self.window_w = w;
        self.window_h = h;
        self.scale = render::integer_scale(w, h);

        let pw = render::WIDTH as i32 * self.scale;
        let ph = render::HEIGHT as i32 * self.scale;
        let ox = (w - pw) / 2;
        let oy = (h - ph) / 2;

        let left = ox as f32 / w as f32 * 2.0 - 1.0;
        let right = (ox + pw) as f32 / w as f32 * 2.0 - 1.0;
        let top = 1.0 - oy as f32 / h as f32 * 2.0;
        let bottom = 1.0 - (oy + ph) as f32 / h as f32 * 2.0;

        let vertices: [Vertex; 4] = [
            Vertex {
                x: left,
                y: top,
                u: 0.0,
                v: 0.0,
            },
            Vertex {
                x: right,
                y: top,
                u: 1.0,
                v: 0.0,
            },
            Vertex {
                x: right,
                y: bottom,
                u: 1.0,
                v: 1.0,
            },
            Vertex {
                x: left,
                y: bottom,
                u: 0.0,
                v: 1.0,
            },
        ];

        self.ctx
            .buffer_update(self.quad, BufferSource::slice(&vertices));
    }

    /// Upload the indexed framebuffer and present it, scaled to the screen.
    /// The palette lookup happens in the shader.
    pub fn present(&mut self, framebuffer: &render::Framebuffer) {
        let indices = framebuffer.pixels();

        if self.index_bpp == 1 {
            self.ctx.texture_update(self.index_texture, indices);
        } else {
            // GLES2/WebGL1 cannot do R8 textures: replicate each index across
            // RGB. The shader reads `.r`, so the two paths sample identically.
            let scratch = &mut self.rgb_scratch;
            for (dst, &idx) in scratch.chunks_exact_mut(3).zip(indices.iter()) {
                dst[0] = idx;
                dst[1] = idx;
                dst[2] = idx;
            }
            self.ctx.texture_update(self.index_texture, scratch);
        }

        // Letterbox bars blend into the frame: must match game::BG_COLOR
        // (13, 13, 18). Kept literal to avoid a render->game dependency.
        self.ctx
            .begin_default_pass(miniquad::PassAction::clear_color(
                13.0 / 255.0,
                13.0 / 255.0,
                18.0 / 255.0,
                1.0,
            ));

        self.ctx.apply_pipeline(&self.pipeline);
        self.ctx.apply_bindings(&self.bindings);
        self.ctx.draw(0, 6, 1);

        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }

    pub fn scale(&self) -> i32 {
        self.scale
    }
}
