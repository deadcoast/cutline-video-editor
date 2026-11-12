//! Narrow libmpv ABI boundary. The library and client outlive the render context.
//! All render operations run on the eframe GL thread; mpv notifications only request a repaint.
use crate::model::Crop;
use anyhow::{Context, Result, bail};
use eframe::{
    egui,
    glow::{self, HasContext},
};
use libloading::Library;
use parking_lot::Mutex;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    num::NonZeroU32,
    path::Path,
    ptr,
    sync::Arc,
    thread::{self, ThreadId},
};

type Handle = *mut c_void;
#[repr(C)]
struct Param {
    kind: c_int,
    data: *mut c_void,
}
#[repr(C)]
struct Init {
    get_proc: unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void,
    context: *mut c_void,
}
#[repr(C)]
struct Fbo {
    fbo: c_int,
    width: c_int,
    height: c_int,
    internal_format: c_int,
}
#[repr(C)]
struct MpvEvent {
    id: c_int,
    error: c_int,
    reply: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

struct Api {
    _library: Library,
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    terminate: unsafe extern "C" fn(Handle),
    option: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, u64, *const *const c_char) -> c_int,
    get: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    set: unsafe extern "C" fn(Handle, u64, *const c_char, c_int, *mut c_void) -> c_int,
    event: unsafe extern "C" fn(Handle, f64) -> *const MpvEvent,
    error: unsafe extern "C" fn(c_int) -> *const c_char,
    render_create: unsafe extern "C" fn(*mut Handle, Handle, *mut Param) -> c_int,
    render_free: unsafe extern "C" fn(Handle),
    render: unsafe extern "C" fn(Handle, *mut Param) -> c_int,
    update: unsafe extern "C" fn(Handle) -> u64,
    callback: unsafe extern "C" fn(Handle, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    swap: unsafe extern "C" fn(Handle),
}
impl Api {
    fn load() -> Result<Self> {
        #[cfg(windows)]
        let candidates = {
            let exe = std::env::current_exe()?;
            let dir = exe.parent().context("Missing application directory")?;
            vec![
                dir.join("mpv-2.dll"),
                dir.join("libmpv-2.dll"),
                dir.join("tools/mpv-2.dll"),
                dir.join("tools/libmpv-2.dll"),
            ]
        };
        #[cfg(not(windows))]
        let candidates = [
            std::path::PathBuf::from("libmpv.so.2"),
            std::path::PathBuf::from("libmpv.so.1"),
        ];
        let lib = candidates.iter().find_map(|p| unsafe { Library::new(p).ok() })
            .context("The playback engine is missing. Extract the complete Cutline ZIP; keep mpv-2.dll and the tools folder beside Cutline.exe.")?;
        // SAFETY: these are stable public libmpv C ABI signatures. Library is retained in Api.
        unsafe {
            macro_rules! symbol {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes())
                        .context(concat!("Missing media function: ", $name))?
                };
            }
            Ok(Self {
                create: symbol!("mpv_create"),
                initialize: symbol!("mpv_initialize"),
                terminate: symbol!("mpv_terminate_destroy"),
                option: symbol!("mpv_set_option_string"),
                command: symbol!("mpv_command_async"),
                get: symbol!("mpv_get_property"),
                set: symbol!("mpv_set_property_async"),
                event: symbol!("mpv_wait_event"),
                error: symbol!("mpv_error_string"),
                render_create: symbol!("mpv_render_context_create"),
                render_free: symbol!("mpv_render_context_free"),
                render: symbol!("mpv_render_context_render"),
                update: symbol!("mpv_render_context_update"),
                callback: symbol!("mpv_render_context_set_update_callback"),
                swap: symbol!("mpv_render_context_report_swap"),
                _library: lib,
            })
        }
    }
    fn check(&self, code: c_int) -> Result<()> {
        if code < 0 {
            bail!(
                "{}",
                unsafe { CStr::from_ptr((self.error)(code)) }.to_string_lossy()
            );
        }
        Ok(())
    }
}

struct Core {
    api: Api,
    handle: Handle,
}
// SAFETY: libmpv's public client API is thread-safe. Render API access is separately thread-checked.
unsafe impl Send for Core {}
unsafe impl Sync for Core {}
impl Drop for Core {
    fn drop(&mut self) {
        unsafe {
            (self.api.terminate)(self.handle);
        }
    }
}

pub struct Player {
    core: Arc<Core>,
    pub renderer: Arc<Mutex<Renderer>>,
    pub loaded: bool,
}
#[derive(Default)]
pub struct Playback {
    pub position: f64,
    pub paused: bool,
    pub eof: bool,
    pub dropped: i64,
}
impl Player {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let api = Api::load()?;
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            bail!("Could not initialize the playback engine");
        }
        let core = Arc::new(Core { api, handle });
        let renderer_name = cc
            .gl
            .as_ref()
            .map(|gl| unsafe { gl.get_parameter_string(glow::RENDERER) })
            .unwrap_or_default();
        let software_renderer = ["llvmpipe", "softpipe", "software rasterizer"]
            .iter()
            .any(|name| renderer_name.to_ascii_lowercase().contains(name));
        for (key, value) in [
            ("vo", "libmpv"),
            // mpv's documented compatibility path avoids broken intermediate FBO
            // passes on the headless software driver. Hardware GPUs retain the
            // normal high-quality renderer, including HDR tone mapping.
            (
                "gpu-dumb-mode",
                if software_renderer { "yes" } else { "auto" },
            ),
            (
                "hwdec",
                if cfg!(windows) {
                    "auto-copy"
                } else {
                    "auto-safe"
                },
            ),
            ("keep-open", "always"),
            // The render target already has the display aspect; fill it exactly instead of
            // letting a rounded target size add one-pixel bars.
            ("keepaspect", "no"),
            ("idle", "yes"),
            ("config", "no"),
            ("input-default-bindings", "no"),
            ("input-vo-keyboard", "no"),
            ("osc", "no"),
            ("terminal", "no"),
            ("video-sync", "audio"),
            ("audio-client-name", "Cutline"),
        ] {
            core.api.check(unsafe {
                (core.api.option)(
                    handle,
                    CString::new(key)?.as_ptr(),
                    CString::new(value)?.as_ptr(),
                )
            })?;
        }
        if let Ok(path) = std::env::var("CUTLINE_MPV_LOG") {
            core.api.check(unsafe {
                (core.api.option)(handle, c"log-file".as_ptr(), CString::new(path)?.as_ptr())
            })?;
            core.api.check(unsafe {
                (core.api.option)(handle, c"msg-level".as_ptr(), c"all=debug".as_ptr())
            })?;
        }
        core.api.check(unsafe { (core.api.initialize)(handle) })?;
        let renderer = Renderer::new(core.clone(), cc)?;
        Ok(Self {
            core,
            renderer: Arc::new(Mutex::new(renderer)),
            loaded: false,
        })
    }
    pub fn command(&self, arguments: &[&str]) -> Result<()> {
        let strings = arguments
            .iter()
            .map(|s| CString::new(*s))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut pointers = strings.iter().map(|s| s.as_ptr()).collect::<Vec<_>>();
        pointers.push(ptr::null());
        self.core
            .api
            .check(unsafe { (self.core.api.command)(self.core.handle, 0, pointers.as_ptr()) })
    }
    pub fn load(&mut self, path: &Path) -> Result<()> {
        let value = path
            .to_str()
            .context("This filename cannot be represented as Unicode")?;
        self.command(&["loadfile", value, "replace"])?;
        self.pause(true)?;
        self.loaded = false;
        Ok(())
    }
    pub fn pause(&self, value: bool) -> Result<()> {
        let mut flag = c_int::from(value);
        self.core.api.check(unsafe {
            (self.core.api.set)(
                self.core.handle,
                0,
                c"pause".as_ptr(),
                3,
                (&mut flag as *mut c_int).cast(),
            )
        })
    }
    pub fn seek(&self, seconds: f64, exact: bool) -> Result<()> {
        self.command(&[
            "seek",
            &format!("{seconds:.9}"),
            if exact {
                "absolute+exact"
            } else {
                "absolute+keyframes"
            },
        ])
    }
    pub fn volume(&self, volume: f64) -> Result<()> {
        self.command(&["set", "volume", &volume.to_string()])
    }
    pub fn rate(&self, speed: f64) -> Result<()> {
        self.command(&["set", "speed", &speed.to_string()])
    }
    pub fn crop_reset(&self) -> Result<()> {
        self.command(&["set", "video-pan-x", "0"])
    }
    pub fn poll(&mut self) -> Result<Playback> {
        if let Some(error) = self.renderer.lock().error.take() {
            bail!("{error}");
        }
        for _ in 0..100 {
            let event = unsafe { &*(self.core.api.event)(self.core.handle, 0.0) };
            if event.id == 0 {
                break;
            }
            if event.id == 8 {
                self.loaded = true;
            }
            if event.id == 7 && !event.data.is_null() {
                let end = unsafe { &*event.data.cast::<EndFile>() };
                if end.reason == 4 {
                    self.core.api.check(end.error)?;
                }
            }
            if event.id == 5 && event.error < 0 {
                self.core.api.check(event.error)?;
            }
        }
        let mut state = Playback {
            paused: true,
            ..Default::default()
        };
        unsafe {
            (self.core.api.get)(
                self.core.handle,
                c"time-pos".as_ptr(),
                5,
                (&mut state.position as *mut f64).cast(),
            );
            let mut pause = 1;
            (self.core.api.get)(
                self.core.handle,
                c"pause".as_ptr(),
                3,
                (&mut pause as *mut c_int).cast(),
            );
            state.paused = pause != 0;
            let mut eof = 0;
            (self.core.api.get)(
                self.core.handle,
                c"eof-reached".as_ptr(),
                3,
                (&mut eof as *mut c_int).cast(),
            );
            state.eof = eof != 0;
            (self.core.api.get)(
                self.core.handle,
                c"frame-drop-count".as_ptr(),
                4,
                (&mut state.dropped as *mut i64).cast(),
            );
        }
        Ok(state)
    }
    pub fn shutdown_renderer(&self) {
        self.renderer.lock().shutdown();
    }
}

pub struct Renderer {
    error: Option<String>,

    core: Arc<Core>,
    render: Handle,
    thread: ThreadId,
    gl: Arc<glow::Context>,
    notification: Box<egui::Context>,
    target: Option<(glow::Framebuffer, glow::Texture, [i32; 2])>,
}
// SAFETY: egui requires Send for paint callbacks. Methods check the original GL thread before
// touching the context. The callback is only invoked by eframe's native painter on that thread.
unsafe impl Send for Renderer {}
impl Renderer {
    fn new(core: Arc<Core>, cc: &eframe::CreationContext<'_>) -> Result<Self> {
        let gl = cc.gl.clone().context("OpenGL is unavailable")?;
        let resolver = cc
            .get_proc_address
            .context("OpenGL loader is unavailable")?;
        unsafe extern "C" fn get_proc(context: *mut c_void, name: *const c_char) -> *mut c_void {
            let f = unsafe { &*(context as *const &dyn Fn(&CStr) -> *const c_void) };
            f(unsafe { CStr::from_ptr(name) }) as *mut c_void
        }
        let mut init = Init {
            get_proc,
            context: (&resolver as *const _ as *mut c_void),
        };
        let mut render = ptr::null_mut();
        let mut params = [
            Param {
                kind: 1,
                data: c"opengl".as_ptr() as *mut c_void,
            },
            Param {
                kind: 2,
                data: (&mut init as *mut Init).cast(),
            },
            Param {
                kind: 0,
                data: ptr::null_mut(),
            },
        ];
        // The GL resolver is used synchronously during creation. mpv retains resolved pointers.
        core.api.check(unsafe {
            (core.api.render_create)(&mut render, core.handle, params.as_mut_ptr())
        })?;
        let mut notification = Box::new(cc.egui_ctx.clone());
        unsafe extern "C" fn wake(context: *mut c_void) {
            let ctx = unsafe { &*context.cast::<egui::Context>() };
            ctx.request_repaint();
        }
        unsafe {
            (core.api.callback)(
                render,
                Some(wake),
                (&mut *notification as *mut egui::Context).cast(),
            );
        }
        Ok(Self {
            error: None,
            core,
            render,
            thread: thread::current().id(),
            gl,
            notification,
            target: None,
        })
    }
    pub fn paint(&mut self, info: egui::PaintCallbackInfo, source: [u32; 2], crop: Crop) {
        assert_eq!(thread::current().id(), self.thread);
        if self.render.is_null() {
            return;
        }
        let gl = self.gl.clone();
        let max_texture = unsafe { gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) }.min(8192) as f32;
        let view = info.viewport_in_pixels();
        // Render the frame at the pixel density the viewport shows it, never above source
        // resolution: a 3840×2160 source in a 1120-pixel window costs mpv 0.7 megapixels per
        // frame instead of 8.3, and mpv's scaler does the reduction instead of a bilinear blit.
        let density = (view.width_px as f32 / crop.width as f32)
            .max(view.height_px as f32 / crop.height as f32);
        let scale = density
            .min(max_texture / source[0].max(source[1]) as f32)
            .min(1.0);
        let size = [
            ((source[0] as f32 * scale).round() as i32).max(2),
            ((source[1] as f32 * scale).round() as i32).max(2),
        ];
        let ratio = [
            size[0] as f32 / source[0] as f32,
            size[1] as f32 / source[1] as f32,
        ];
        unsafe {
            let old_framebuffer =
                NonZeroU32::new(gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING) as u32)
                    .map(glow::NativeFramebuffer);
            let mut stale = false;
            if self.target.as_ref().is_none_or(|t| t.2 != size) {
                stale = true;
                if let Some((f, t, _)) = self.target.take() {
                    gl.delete_framebuffer(f);
                    gl.delete_texture(t);
                }
                let Ok(fbo) = gl.create_framebuffer() else {
                    self.error =
                        Some("The graphics driver could not allocate a video surface.".into());
                    return;
                };
                let Ok(texture) = gl.create_texture() else {
                    gl.delete_framebuffer(fbo);
                    self.error =
                        Some("There is not enough graphics memory for the video preview.".into());
                    return;
                };
                gl.bind_texture(glow::TEXTURE_2D, Some(texture));
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA8 as i32,
                    size[0],
                    size[1],
                    0,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(None),
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::LINEAR as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::LINEAR as i32,
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    Some(texture),
                    0,
                );
                if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, old_framebuffer);
                    gl.delete_framebuffer(fbo);
                    gl.delete_texture(texture);
                    self.error = Some("This graphics driver cannot render the video surface. Update the graphics driver and reopen Cutline.".into());
                    return;
                }
                self.target = Some((fbo, texture, size));
            }
            let Some((fbo, _, _)) = self.target else {
                return;
            };
            // mpv reports whether the frame changed since the last render. Interface repaints
            // (pointer motion, the paused 10 Hz tick) reuse the retained target and only blit.
            const UPDATE_FRAME: u64 = 1;
            let changed = (self.core.api.update)(self.render) & UPDATE_FRAME != 0;
            if changed || stale {
                self.render_frame(fbo, size);
            }
            gl.disable(glow::BLEND);
            gl.color_mask(true, true, true, true);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, old_framebuffer);
            let x = (crop.x as f32 * ratio[0]).round() as i32;
            let y = ((source[1] - crop.y - crop.height) as f32 * ratio[1]).round() as i32;
            let w = ((crop.width as f32 * ratio[0]).round() as i32).max(1);
            let h = ((crop.height as f32 * ratio[1]).round() as i32).max(1);
            let clip = info.clip_rect_in_pixels();
            gl.enable(glow::SCISSOR_TEST);
            gl.scissor(
                clip.left_px,
                clip.from_bottom_px,
                clip.width_px,
                clip.height_px,
            );
            gl.blit_framebuffer(
                x,
                y,
                x + w,
                y + h,
                view.left_px,
                view.from_bottom_px,
                view.left_px + view.width_px,
                view.from_bottom_px + view.height_px,
                glow::COLOR_BUFFER_BIT,
                glow::LINEAR,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, old_framebuffer);
        }
    }
    /// Draws mpv's current frame into the retained target. GL thread only (checked by `paint`).
    unsafe fn render_frame(&mut self, fbo: glow::Framebuffer, size: [i32; 2]) {
        let gl = self.gl.clone();
        unsafe {
            // libmpv requires the caller's OpenGL state to be reset to defaults.
            // egui leaves a shader, vertex array, blend state and texture bindings active.
            gl.use_program(None);
            gl.bind_vertex_array(None);
            gl.bind_buffer(glow::ARRAY_BUFFER, None);
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.bind_sampler(0, None);
            for flag in [
                glow::SCISSOR_TEST,
                glow::BLEND,
                glow::CULL_FACE,
                glow::DEPTH_TEST,
                glow::STENCIL_TEST,
            ] {
                gl.disable(flag);
            }
            gl.color_mask(true, true, true, true);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
            let mut target = Fbo {
                fbo: fbo.0.get() as i32,
                width: size[0],
                height: size[1],
                internal_format: glow::RGBA8 as i32,
            };
            let mut flip = 1i32;
            let mut block = 0i32;
            let mut params = [
                Param {
                    kind: 3,
                    data: (&mut target as *mut Fbo).cast(),
                },
                Param {
                    kind: 4,
                    data: (&mut flip as *mut i32).cast(),
                },
                Param {
                    kind: 12,
                    data: (&mut block as *mut i32).cast(),
                },
                Param {
                    kind: 0,
                    data: ptr::null_mut(),
                },
            ];
            let code = (self.core.api.render)(self.render, params.as_mut_ptr());
            if code < 0 {
                self.error = Some(format!(
                    "Video rendering failed ({code}). Reopen the file or update the graphics driver."
                ));
            }
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                self.error = Some(format!(
                    "The graphics driver reported a rendering error ({error:#x})."
                ));
            }
            (self.core.api.swap)(self.render);
        }
    }
    pub fn shutdown(&mut self) {
        assert_eq!(thread::current().id(), self.thread);
        if self.render.is_null() {
            return;
        }
        unsafe {
            (self.core.api.callback)(self.render, None, ptr::null_mut());
            (self.core.api.render_free)(self.render);
            self.render = ptr::null_mut();
            if let Some((f, t, _)) = self.target.take() {
                self.gl.delete_framebuffer(f);
                self.gl.delete_texture(t);
            }
        }
        // Keep callback userdata alive until after render_free synchronized with the decoder.
        let _ = &self.notification;
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        self.shutdown();
    }
}
