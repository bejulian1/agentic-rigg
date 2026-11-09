//! GL-delen av 3D-kortet (port av `web/js/lys3d/gl/renderer.js`), med
//! `glow` mot GLES 3.0 i en `GtkGLArea`. Funksjonspekerne kommer fra
//! libepoxy, som GTK alt har lastet.
//!
//! Passene i ett bilde (som demoen):
//! 1. Skyggekart fra hovedlyset (bare når noe har flyttet seg).
//! 2. Bakgrunnslys bak kortet (valgfritt).
//! 3. Ugjennomsiktige deler: tre lys, halvkule-omgivelse, refleksjon,
//!    PCF-skygge, lyset fra LED-sonene og LED-ene selv. ACES og gamma.
//! 4. Gulvet. 5. Lag som tones, bakfra. 6. Glorie rundt hver LED.
//!
//! Alt tegnes i en egen buffer med 4 × MSAA og kopieres til GLArea-ens
//! buffer til slutt. Plukking (delen under pekeren) tegner bare én piksel.

use super::mesh::MeshData;
use super::model::Mat;
use super::textures::{Image, Tex};
use glam::{Mat3, Mat4, Vec3, Vec4};
use glow::HasContext;
use std::collections::HashMap;
use std::ffi::{CStr, c_void};

const SHADOW: i32 = 2048;
const NO_HOLES: [f32; 8] = [0.0; 8];

fn src(name: &str) -> String {
    let s = match name {
        "main.vert" => include_str!("../../../../data/lys3d/main.vert"),
        "main.frag" => include_str!("../../../../data/lys3d/main.frag"),
        "depth.vert" => include_str!("../../../../data/lys3d/depth.vert"),
        "depth.frag" => include_str!("../../../../data/lys3d/depth.frag"),
        "floor.vert" => include_str!("../../../../data/lys3d/floor.vert"),
        "floor.frag" => include_str!("../../../../data/lys3d/floor.frag"),
        "quad.vert" => include_str!("../../../../data/lys3d/quad.vert"),
        "ambient.frag" => include_str!("../../../../data/lys3d/ambient.frag"),
        "glow.vert" => include_str!("../../../../data/lys3d/glow.vert"),
        _ => include_str!("../../../../data/lys3d/glow.frag"),
    };
    s.replace("#include lys.glsl", include_str!("../../../../data/lys3d/lys.glsl"))
}

/// Funksjonspekerne fra libepoxy (`epoxy_glClear` osv. er pekervariabler).
fn epoxy(name: &CStr) -> *const c_void {
    let Ok(n) = name.to_str() else { return std::ptr::null() };
    let Ok(sym) = std::ffi::CString::new(format!("epoxy_{n}")) else { return std::ptr::null() };
    // SAFETY: dlsym slår bare opp et navn i prosessen. libepoxy eksporterer
    // hver GL-funksjon som en variabel med en funksjonspeker; vi leser den.
    unsafe {
        let p = libc::dlsym(libc::RTLD_DEFAULT, sym.as_ptr());
        if p.is_null() { std::ptr::null() } else { *(p as *const *const c_void) }
    }
}

pub fn context() -> glow::Context {
    // SAFETY: kalles bare med GLArea-ens kontekst gjort gjeldende.
    unsafe { glow::Context::from_loader_function_cstr(epoxy) }
}

fn bytes<T>(v: &[T]) -> &[u8] {
    // SAFETY: f32 og u32 har ingen utfylling; vi leser bare minnet som byte.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

struct Prog {
    p: glow::NativeProgram,
    u: HashMap<String, glow::UniformLocation>,
}

impl Prog {
    fn u(&self, n: &str) -> Option<&glow::UniformLocation> {
        self.u.get(n)
    }
}

pub struct GpuMesh {
    vao: glow::NativeVertexArray,
    bufs: Vec<glow::NativeBuffer>,
    count: i32,
}

/// Lysene (følger kameraets dreining).
pub struct Lights {
    pub key: [f32; 3],
    pub key_col: [f32; 3],
    pub fill: [f32; 3],
    pub fill_col: [f32; 3],
    pub rim: [f32; 3],
    pub rim_col: [f32; 3],
    pub sky: [f32; 3],
    pub ground: [f32; 3],
    pub exposure: f32,
    pub env: f32,
}

pub struct Leds {
    pub colors: [f32; 168],
    pub zone_gain: [f32; 3],
    pub bar_a: [f32; 3],
    pub bar_b: [f32; 3],
    pub bar_col: [f32; 3],
    pub f1_pos: [f32; 3],
    pub f1_col: [f32; 3],
    pub f2_pos: [f32; 3],
    pub f2_col: [f32; 3],
}

pub struct PartDraw {
    pub mesh: usize,
    pub mat: Mat,
    pub model: Mat4,
    pub nmat: Mat3,
    pub alpha: f32,
    pub visible: bool,
    pub highlight: f32,
    pub depth: f32,
    pub pick_id: u32,
    pub cast: bool,
}

pub struct Frame {
    pub parts: Vec<PartDraw>,
    pub view: Mat4,
    pub proj: Mat4,
    pub view_proj: Mat4,
    pub cam_pos: [f32; 3],
    pub lights: Lights,
    pub leds: Leds,
    pub accent: [f32; 3],
    pub glow: [f32; 56 * 6],
    pub glow_count: i32,
    pub floor_center: [f32; 3],
    pub floor_alpha: f32,
    pub ambient: Option<([f32; 2], [f32; 3], f32)>,
    pub bg: [f32; 3],
    pub scene_center: [f32; 3],
    pub scene_radius: f32,
    pub shadow_dirty: bool,
}

struct Msaa {
    fbo: glow::NativeFramebuffer,
    color: glow::NativeRenderbuffer,
    depth: glow::NativeRenderbuffer,
    w: i32,
    h: i32,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub frames: u64,
    pub draws: u32,
    pub shadow_passes: u64,
}

pub struct Renderer {
    gl: glow::Context,
    main: Prog,
    depth: Prog,
    floor: Prog,
    ambient: Prog,
    glowp: Prog,
    shadow_tex: glow::NativeTexture,
    shadow_fbo: glow::NativeFramebuffer,
    pick_fbo: glow::NativeFramebuffer,
    pick_rb: [glow::NativeRenderbuffer; 2],
    quad_vao: glow::NativeVertexArray,
    quad_buf: glow::NativeBuffer,
    glow_vao: glow::NativeVertexArray,
    glow_buf: glow::NativeBuffer,
    msaa: Option<Msaa>,
    samples: i32,
    lvp: Mat4,
    pub meshes: Vec<GpuMesh>,
    textures: HashMap<Tex, glow::NativeTexture>,
    pub stats: Stats,
    pub info: String,
}

fn compile(gl: &glow::Context, vs: &str, fs: &str) -> Result<Prog, String> {
    // SAFETY: GL-kall med gjeldende kontekst; ressursene eies av Renderer.
    unsafe {
        let p = gl.create_program()?;
        let mut shaders = Vec::new();
        for (ty, s) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let sh = gl.create_shader(ty)?;
            gl.shader_source(sh, &src(s));
            gl.compile_shader(sh);
            if !gl.get_shader_compile_status(sh) {
                return Err(format!("{s}: {}", gl.get_shader_info_log(sh)));
            }
            gl.attach_shader(p, sh);
            shaders.push(sh);
        }
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            return Err(format!("{vs}+{fs}: {}", gl.get_program_info_log(p)));
        }
        for sh in shaders {
            gl.detach_shader(p, sh);
            gl.delete_shader(sh);
        }
        let mut u = HashMap::new();
        for i in 0..gl.get_active_uniforms(p) {
            if let Some(a) = gl.get_active_uniform(p, i)
                && let Some(loc) = gl.get_uniform_location(p, &a.name)
            {
                u.insert(a.name.trim_end_matches("[0]").to_string(), loc);
            }
        }
        Ok(Prog { p, u })
    }
}

impl Renderer {
    /// Lager programmene og bufferne. Feiler hvis skyggeleggerne ikke kompilerer.
    pub fn new(gl: glow::Context) -> Result<Renderer, String> {
        // SAFETY: GL-kall med gjeldende kontekst.
        unsafe {
            let info = format!("{} · {}", gl.get_parameter_string(glow::RENDERER), gl.get_parameter_string(glow::VERSION));
            let main = compile(&gl, "main.vert", "main.frag")?;
            let depth = compile(&gl, "depth.vert", "depth.frag")?;
            let floor = compile(&gl, "floor.vert", "floor.frag")?;
            let ambient = compile(&gl, "quad.vert", "ambient.frag")?;
            let glowp = compile(&gl, "glow.vert", "glow.frag")?;
            // Skyggekart: dybdetekstur med sammenligning (maskinvare-PCF).
            let shadow_tex = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(shadow_tex));
            gl.tex_storage_2d(glow::TEXTURE_2D, 1, glow::DEPTH_COMPONENT24, SHADOW, SHADOW);
            for (k, v) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE),
                (glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, k, v as i32);
            }
            let shadow_fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(shadow_fbo));
            gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(shadow_tex), 0);
            gl.draw_buffers(&[glow::NONE]);
            gl.read_buffer(glow::NONE);
            // Plukking: 1 × 1 piksel med farge og dybde.
            let pc = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(pc));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, 1, 1);
            let pd = gl.create_renderbuffer()?;
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(pd));
            gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT24, 1, 1);
            let pick_fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(pick_fbo));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(pc));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(pd));
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            // Firkant (−1…1) for gulv, bakgrunnslys og glorier.
            let quad_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(quad_vao));
            let quad_buf = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_buf));
            let q: [f32; 12] = [-1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, 1.0];
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(&q), glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
            let glow_vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(glow_vao));
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(quad_buf));
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 0, 0);
            let glow_buf = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(glow_buf));
            gl.buffer_data_size(glow::ARRAY_BUFFER, 56 * 6 * 4, glow::DYNAMIC_DRAW);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, 24, 0);
            gl.vertex_attrib_divisor(1, 1);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, 24, 12);
            gl.vertex_attrib_divisor(2, 1);
            gl.bind_vertex_array(None);
            let samples = gl.get_parameter_i32(glow::MAX_SAMPLES).clamp(0, 4);
            Ok(Renderer {
                gl,
                main,
                depth,
                floor,
                ambient,
                glowp,
                shadow_tex,
                shadow_fbo,
                pick_fbo,
                pick_rb: [pc, pd],
                quad_vao,
                quad_buf,
                glow_vao,
                glow_buf,
                msaa: None,
                samples,
                lvp: Mat4::IDENTITY,
                meshes: Vec::new(),
                textures: HashMap::new(),
                stats: Stats::default(),
                info,
            })
        }
    }

    pub fn add_mesh(&mut self, d: &MeshData) -> Result<usize, String> {
        let gl = &self.gl;
        // SAFETY: GL-kall med gjeldende kontekst.
        unsafe {
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let mut bufs = Vec::new();
            for (loc, data, size) in [(0u32, &d.pos, 3), (1, &d.nrm, 3), (2, &d.uv, 2), (3, &d.led, 1)] {
                let b = gl.create_buffer()?;
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(b));
                gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes(data), glow::STATIC_DRAW);
                gl.enable_vertex_attrib_array(loc);
                gl.vertex_attrib_pointer_f32(loc, size, glow::FLOAT, false, 0, 0);
                bufs.push(b);
            }
            let ib = gl.create_buffer()?;
            gl.bind_buffer(glow::ELEMENT_ARRAY_BUFFER, Some(ib));
            gl.buffer_data_u8_slice(glow::ELEMENT_ARRAY_BUFFER, bytes(&d.idx), glow::STATIC_DRAW);
            bufs.push(ib);
            gl.bind_vertex_array(None);
            self.meshes.push(GpuMesh { vao, bufs, count: d.idx.len() as i32 });
            Ok(self.meshes.len() - 1)
        }
    }

    /// RGBA8 med mipmaps og anisotropi. Fargene er sRGB og gjøres lineære i skyggeleggeren.
    pub fn add_texture(&mut self, img: &Image) -> Result<(), String> {
        let gl = &self.gl;
        // SAFETY: GL-kall med gjeldende kontekst; dataene lever under kallet.
        unsafe {
            let t = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, img.w, img.h, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&img.rgba)));
            gl.generate_mipmap(glow::TEXTURE_2D);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
            let wrap = if img.repeat { glow::REPEAT } else { glow::CLAMP_TO_EDGE } as i32;
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, wrap);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, wrap);
            if gl.supported_extensions().contains("GL_EXT_texture_filter_anisotropic") {
                let max = gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT);
                gl.tex_parameter_f32(glow::TEXTURE_2D, glow::TEXTURE_MAX_ANISOTROPY_EXT, max.min(8.0));
            }
            self.textures.insert(img.tex, t);
            Ok(())
        }
    }

    fn ensure_msaa(&mut self, w: i32, h: i32) {
        if self.samples < 2 || self.msaa.as_ref().is_some_and(|m| m.w == w && m.h == h) {
            return;
        }
        let gl = &self.gl;
        // SAFETY: GL-kall med gjeldende kontekst.
        unsafe {
            if let Some(m) = self.msaa.take() {
                gl.delete_framebuffer(m.fbo);
                gl.delete_renderbuffer(m.color);
                gl.delete_renderbuffer(m.depth);
            }
            let (Ok(fbo), Ok(color), Ok(depth)) = (gl.create_framebuffer(), gl.create_renderbuffer(), gl.create_renderbuffer()) else { return };
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(color));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, self.samples, glow::RGBA8, w, h);
            gl.bind_renderbuffer(glow::RENDERBUFFER, Some(depth));
            gl.renderbuffer_storage_multisample(glow::RENDERBUFFER, self.samples, glow::DEPTH_COMPONENT24, w, h);
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(color));
            gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(depth));
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                gl.delete_framebuffer(fbo);
                gl.delete_renderbuffer(color);
                gl.delete_renderbuffer(depth);
                self.samples = 0;
                return;
            }
            self.msaa = Some(Msaa { fbo, color, depth, w, h });
        }
    }

    /// Lysets matrise: ortografisk fra hovedlysets retning, tilpasset scenens kule.
    fn light_matrix(&mut self, key: [f32; 3], center: [f32; 3], radius: f32) {
        let k = Vec3::from(key);
        let c = Vec3::from(center);
        let eye = c + k * radius * 2.0;
        let up = if key[1].abs() > 0.95 { Vec3::Z } else { Vec3::Y };
        let view = Mat4::look_at_rh(eye, c, up);
        let proj = Mat4::orthographic_rh_gl(-radius, radius, -radius, radius, radius * 0.5, radius * 3.5);
        self.lvp = proj * view;
    }

    unsafe fn set_material(&self, prog: &Prog, m: &Mat) {
        let gl = &self.gl;
        // SAFETY: kalleren har gjort konteksten gjeldende.
        unsafe {
            if let Some(u) = prog.u("u_color") {
                gl.uniform_3_f32_slice(Some(u), &m.color);
            }
            if let Some(u) = prog.u("u_rough") {
                gl.uniform_1_f32(Some(u), m.rough);
            }
            if let Some(u) = prog.u("u_metal") {
                gl.uniform_1_f32(Some(u), m.metal);
            }
            if let Some(u) = prog.u("u_double") {
                gl.uniform_1_f32(Some(u), if m.double { 1.0 } else { 0.0 });
            }
            let tex = m.map.and_then(|t| self.textures.get(&t));
            if let Some(u) = prog.u("u_useMap") {
                gl.uniform_1_f32(Some(u), if tex.is_some() { 1.0 } else { 0.0 });
            }
            if let Some(u) = prog.u("u_alphaTest") {
                gl.uniform_1_f32(Some(u), m.alpha_test);
            }
            if let Some(t) = tex {
                gl.active_texture(glow::TEXTURE0);
                gl.bind_texture(glow::TEXTURE_2D, Some(*t));
            }
            if let Some(u) = prog.u("u_ledDiffuse") {
                gl.uniform_1_f32(Some(u), if m.led { 1.0 } else { 0.0 });
            }
            if let Some(u) = prog.u("u_holes") {
                gl.uniform_4_f32_slice(Some(u), &m.holes.unwrap_or(NO_HOLES));
            }
        }
    }

    unsafe fn set_led_lights(&self, prog: &Prog, l: &Leds) {
        let gl = &self.gl;
        // SAFETY: kalleren har gjort konteksten gjeldende.
        unsafe {
            for (n, v) in [("u_barA", l.bar_a), ("u_barB", l.bar_b), ("u_barCol", l.bar_col), ("u_f1Pos", l.f1_pos), ("u_f1Col", l.f1_col), ("u_f2Pos", l.f2_pos), ("u_f2Col", l.f2_col)] {
                if let Some(u) = prog.u(n) {
                    gl.uniform_3_f32_slice(Some(u), &v);
                }
            }
        }
    }

    fn fbo_binding(&self) -> Option<glow::NativeFramebuffer> {
        // SAFETY: lesing av GL-tilstand med gjeldende kontekst.
        let id = unsafe { self.gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) };
        std::num::NonZeroU32::new(id as u32).map(glow::NativeFramebuffer)
    }

    /// Tegner ett bilde i GLArea-ens buffer (bundet av GTK), `w` × `h` piksler.
    pub fn render(&mut self, f: &Frame, w: i32, h: i32) {
        let target = self.fbo_binding();
        self.ensure_msaa(w, h);
        self.stats.frames += 1;
        self.stats.draws = 0;
        if f.shadow_dirty {
            self.light_matrix(f.lights.key, f.scene_center, f.scene_radius);
        }
        let gl = &self.gl;
        // SAFETY: GL-kall med gjeldende kontekst (render-signalet til GLArea).
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.depth_func(glow::LEQUAL);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::SCISSOR_TEST);
            // 1. Skyggekart
            if f.shadow_dirty {
                self.stats.shadow_passes += 1;
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.shadow_fbo));
                gl.viewport(0, 0, SHADOW, SHADOW);
                gl.depth_mask(true);
                gl.clear(glow::DEPTH_BUFFER_BIT);
                gl.color_mask(false, false, false, false);
                gl.enable(glow::POLYGON_OFFSET_FILL);
                gl.polygon_offset(1.6, 3.0);
                let p = &self.depth;
                gl.use_program(Some(p.p));
                gl.uniform_matrix_4_f32_slice(p.u("u_vp"), false, &self.lvp.to_cols_array());
                gl.uniform_1_i32(p.u("u_map"), 0);
                for part in &f.parts {
                    if !part.cast || part.alpha < 0.5 || !part.visible {
                        continue;
                    }
                    gl.uniform_matrix_4_f32_slice(p.u("u_model"), false, &part.model.to_cols_array());
                    self.set_material(p, &part.mat);
                    let m = &self.meshes[part.mesh];
                    gl.bind_vertex_array(Some(m.vao));
                    gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
                }
                gl.disable(glow::POLYGON_OFFSET_FILL);
            }
            // Selve bildet: i MSAA-bufferen når den finnes.
            let draw_fbo = self.msaa.as_ref().map(|m| m.fbo).or(target);
            gl.bind_framebuffer(glow::FRAMEBUFFER, draw_fbo);
            gl.viewport(0, 0, w, h);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.clear_color(f.bg[0], f.bg[1], f.bg[2], 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            // Alfa skrives ikke: GTK skal se et ugjennomsiktig bilde.
            gl.color_mask(true, true, true, false);

            // 2. Bakgrunnslys
            if let Some((c, col, a)) = f.ambient
                && a > 0.002
            {
                let p = &self.ambient;
                gl.use_program(Some(p.p));
                gl.disable(glow::DEPTH_TEST);
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE);
                gl.uniform_2_f32_slice(p.u("u_c"), &c);
                gl.uniform_2_f32_slice(p.u("u_r"), &[0.34, 0.5]);
                gl.uniform_3_f32_slice(p.u("u_col"), &col);
                gl.uniform_1_f32(p.u("u_alpha"), a);
                gl.bind_vertex_array(Some(self.quad_vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 6);
                gl.disable(glow::BLEND);
                gl.enable(glow::DEPTH_TEST);
            }

            // 3. Delene
            let p = &self.main;
            gl.use_program(Some(p.p));
            gl.uniform_matrix_4_f32_slice(p.u("u_vp"), false, &f.view_proj.to_cols_array());
            gl.uniform_matrix_4_f32_slice(p.u("u_lvp"), false, &self.lvp.to_cols_array());
            gl.uniform_3_f32_slice(p.u("u_cam"), &f.cam_pos);
            let l = &f.lights;
            for (n, v) in [("u_keyDir", l.key), ("u_keyCol", l.key_col), ("u_fillDir", l.fill), ("u_fillCol", l.fill_col), ("u_rimDir", l.rim), ("u_rimCol", l.rim_col), ("u_sky", l.sky), ("u_ground", l.ground), ("u_accent", f.accent)] {
                gl.uniform_3_f32_slice(p.u(n), &v);
            }
            gl.uniform_1_f32(p.u("u_exposure"), l.exposure);
            gl.uniform_1_f32(p.u("u_env"), l.env);
            gl.uniform_3_f32_slice(p.u("u_led"), &f.leds.colors);
            gl.uniform_3_f32_slice(p.u("u_zoneGain"), &f.leds.zone_gain);
            self.set_led_lights(p, &f.leds);
            gl.uniform_1_i32(p.u("u_map"), 0);
            gl.uniform_1_i32(p.u("u_shadow"), 1);
            gl.uniform_1_f32(p.u("u_shadowTexel"), 1.0 / SHADOW as f32);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(self.shadow_tex));
            gl.active_texture(glow::TEXTURE0);
            let mut draws = 0;
            let draw_part = |part: &PartDraw, draws: &mut u32| {
                gl.uniform_matrix_4_f32_slice(p.u("u_model"), false, &part.model.to_cols_array());
                gl.uniform_matrix_3_f32_slice(p.u("u_nmat"), false, &part.nmat.to_cols_array());
                gl.uniform_1_f32(p.u("u_alpha"), part.alpha);
                gl.uniform_1_f32(p.u("u_highlight"), part.highlight);
                self.set_material(p, &part.mat);
                let m = &self.meshes[part.mesh];
                gl.bind_vertex_array(Some(m.vao));
                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
                *draws += 1;
            };
            let mut fading: Vec<&PartDraw> = Vec::new();
            for part in &f.parts {
                if !part.visible || part.alpha <= 0.004 {
                    continue;
                }
                if part.alpha < 0.999 {
                    fading.push(part);
                } else {
                    draw_part(part, &mut draws);
                }
            }

            // 4. Gulvet
            if f.floor_alpha > 0.01 {
                let fp = &self.floor;
                gl.use_program(Some(fp.p));
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE_MINUS_SRC_ALPHA);
                gl.depth_mask(false);
                gl.uniform_matrix_4_f32_slice(fp.u("u_vp"), false, &f.view_proj.to_cols_array());
                gl.uniform_matrix_4_f32_slice(fp.u("u_lvp"), false, &self.lvp.to_cols_array());
                gl.uniform_3_f32_slice(fp.u("u_center"), &f.floor_center);
                gl.uniform_1_f32(fp.u("u_size"), 330.0);
                gl.uniform_1_f32(fp.u("u_alpha"), f.floor_alpha);
                gl.uniform_1_i32(fp.u("u_shadow"), 1);
                gl.uniform_1_f32(fp.u("u_shadowTexel"), 1.0 / SHADOW as f32);
                self.set_led_lights(fp, &f.leds);
                gl.bind_vertex_array(Some(self.quad_vao));
                gl.draw_arrays(glow::TRIANGLES, 0, 6);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
                gl.use_program(Some(p.p));
            }

            // 5. Lag som tones inn eller ut: bakerst først.
            if !fading.is_empty() {
                fading.sort_by(|a, b| b.depth.total_cmp(&a.depth));
                gl.enable(glow::BLEND);
                gl.blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                for part in fading {
                    draw_part(part, &mut draws);
                }
                gl.disable(glow::BLEND);
            }

            // 6. Glorier
            if f.glow_count > 0 {
                let gp = &self.glowp;
                gl.use_program(Some(gp.p));
                gl.enable(glow::BLEND);
                gl.blend_func(glow::ONE, glow::ONE);
                gl.depth_mask(false);
                gl.uniform_matrix_4_f32_slice(gp.u("u_view"), false, &f.view.to_cols_array());
                gl.uniform_matrix_4_f32_slice(gp.u("u_proj"), false, &f.proj.to_cols_array());
                gl.uniform_1_f32(gp.u("u_size"), 5.2);
                gl.uniform_1_f32(gp.u("u_gain"), 2.2);
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(self.glow_buf));
                gl.buffer_sub_data_u8_slice(glow::ARRAY_BUFFER, 0, bytes(&f.glow[..f.glow_count as usize * 6]));
                gl.bind_vertex_array(Some(self.glow_vao));
                gl.draw_arrays_instanced(glow::TRIANGLES, 0, 6, f.glow_count);
                gl.depth_mask(true);
                gl.disable(glow::BLEND);
            }
            gl.bind_vertex_array(None);
            gl.color_mask(true, true, true, true);
            // Kopier MSAA-bufferen til GLArea-ens buffer.
            if let Some(m) = &self.msaa {
                gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(m.fbo));
                gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, target);
                gl.blit_framebuffer(0, 0, w, h, 0, 0, w, h, glow::COLOR_BUFFER_BIT, glow::NEAREST);
            }
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
            self.stats.draws = draws;
        }
    }

    /// Hvilken del ligger under pikselen (px, py), i bufferens piksler (y ned)?
    /// Tegner bare den ene pikselen: projeksjonen forstørres rundt punktet.
    pub fn pick(&self, f: &Frame, px: i32, py: i32, w: i32, h: i32) -> u32 {
        let target = self.fbo_binding();
        let gl = &self.gl;
        let (wf, hf) = (w.max(1) as f32, h.max(1) as f32);
        let nx = (px as f32 + 0.5) / wf * 2.0 - 1.0;
        let ny = 1.0 - (py as f32 + 0.5) / hf * 2.0;
        let pick_m = Mat4::from_cols(Vec4::new(wf, 0.0, 0.0, 0.0), Vec4::new(0.0, hf, 0.0, 0.0), Vec4::new(0.0, 0.0, 1.0, 0.0), Vec4::new(-nx * wf, -ny * hf, 0.0, 1.0));
        let vp = pick_m * f.view_proj;
        let mut px4 = [0u8; 4];
        // SAFETY: GL-kall med gjeldende kontekst.
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.pick_fbo));
            gl.viewport(0, 0, 1, 1);
            gl.color_mask(true, true, true, true);
            gl.depth_mask(true);
            gl.clear_color(0.0, 0.0, 0.0, 0.0);
            gl.clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
            gl.enable(glow::DEPTH_TEST);
            let p = &self.depth;
            gl.use_program(Some(p.p));
            gl.uniform_matrix_4_f32_slice(p.u("u_vp"), false, &vp.to_cols_array());
            gl.uniform_1_i32(p.u("u_map"), 0);
            for part in &f.parts {
                if !part.visible || part.alpha < 0.5 || part.pick_id == 0 {
                    continue;
                }
                gl.uniform_matrix_4_f32_slice(p.u("u_model"), false, &part.model.to_cols_array());
                gl.uniform_3_f32(p.u("u_id"), (part.pick_id & 255) as f32 / 255.0, ((part.pick_id >> 8) & 255) as f32 / 255.0, 0.0);
                self.set_material(p, &part.mat);
                let m = &self.meshes[part.mesh];
                gl.bind_vertex_array(Some(m.vao));
                gl.draw_elements(glow::TRIANGLES, m.count, glow::UNSIGNED_INT, 0);
            }
            gl.read_pixels(0, 0, 1, 1, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::Slice(Some(&mut px4)));
            gl.bind_vertex_array(None);
            gl.bind_framebuffer(glow::FRAMEBUFFER, target);
        }
        if px4[3] > 0 { px4[0] as u32 + ((px4[1] as u32) << 8) } else { 0 }
    }

    /// Venter til GPU-en er ferdig (bare til målingene).
    pub fn finish(&self) {
        // SAFETY: GL-kall med gjeldende kontekst.
        unsafe { self.gl.finish() };
    }

    /// Frigjør alt (kalles med konteksten gjeldende, i `unrealize`).
    pub fn destroy(mut self) {
        let gl = &self.gl;
        // SAFETY: GL-kall med gjeldende kontekst; ingenting brukes etterpå.
        unsafe {
            for m in self.meshes.drain(..) {
                gl.delete_vertex_array(m.vao);
                for b in m.bufs {
                    gl.delete_buffer(b);
                }
            }
            for (_, t) in self.textures.drain() {
                gl.delete_texture(t);
            }
            if let Some(m) = self.msaa.take() {
                gl.delete_framebuffer(m.fbo);
                gl.delete_renderbuffer(m.color);
                gl.delete_renderbuffer(m.depth);
            }
            for p in [&self.main, &self.depth, &self.floor, &self.ambient, &self.glowp] {
                gl.delete_program(p.p);
            }
            gl.delete_texture(self.shadow_tex);
            gl.delete_framebuffer(self.shadow_fbo);
            gl.delete_framebuffer(self.pick_fbo);
            for r in self.pick_rb {
                gl.delete_renderbuffer(r);
            }
            gl.delete_vertex_array(self.quad_vao);
            gl.delete_vertex_array(self.glow_vao);
            gl.delete_buffer(self.quad_buf);
            gl.delete_buffer(self.glow_buf);
        }
    }
}
