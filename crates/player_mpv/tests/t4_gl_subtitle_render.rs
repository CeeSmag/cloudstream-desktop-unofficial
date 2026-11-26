//! T4: verifies that selecting a subtitle after playback has started actually renders subtitle
//! pixels into the zero-copy OpenGL texture on Windows OpenGL ES contexts (guarding against
//! drivers that advertise `GL_EXT_disjoint_timer_query` but reject `GL_GPU_DISJOINT_EXT` with
//! `GL_INVALID_ENUM`, which otherwise aborts libplacebo's overlay shader compilation).
//!
//! Run with:
//! `CS_LIBMPV_PATH=...\\libmpv-2.dll cargo test -p player_mpv --test t4_gl_subtitle_render -- --ignored --nocapture`
#![allow(
    unsafe_code,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::too_many_lines
)]

#[cfg(windows)]
mod windows_test {
    use std::ffi::{c_char, c_int, c_uint, c_void, CString};
    use std::num::NonZeroU32;
    use std::time::{Duration, Instant};

    use glow::HasContext;
    use player_api::{PlaybackRequest, SubtitleSource, TrackKind, VideoPlayer};
    use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

    #[repr(C)]
    struct PixelFormatDescriptor {
        n_size: u16,
        n_version: u16,
        dw_flags: u32,
        i_pixel_type: u8,
        c_color_bits: u8,
        c_red_bits: u8,
        c_red_shift: u8,
        c_green_bits: u8,
        c_green_shift: u8,
        c_blue_bits: u8,
        c_blue_shift: u8,
        c_alpha_bits: u8,
        c_alpha_shift: u8,
        c_accum_bits: u8,
        c_accum_red_bits: u8,
        c_accum_green_bits: u8,
        c_accum_blue_bits: u8,
        c_accum_alpha_bits: u8,
        c_depth_bits: u8,
        c_stencil_bits: u8,
        c_aux_buffers: u8,
        i_layer_type: u8,
        b_reserved: u8,
        dw_layer_mask: u32,
        dw_visible_mask: u32,
        dw_damage_mask: u32,
    }

    struct WglEsContext {
        _user32: libloading::Library,
        _gdi32: libloading::Library,
        opengl32: libloading::Library,
        wgl_get_proc: unsafe extern "system" fn(*const c_char) -> *const c_void,
    }

    impl WglEsContext {
        fn new() -> Self {
            unsafe {
                let user32 = libloading::Library::new("user32.dll").expect("user32");
                let gdi32 = libloading::Library::new("gdi32.dll").expect("gdi32");
                let opengl32 = libloading::Library::new("opengl32.dll").expect("opengl32");

                type CreateWindowExA = unsafe extern "system" fn(
                    u32,
                    *const c_char,
                    *const c_char,
                    u32,
                    c_int,
                    c_int,
                    c_int,
                    c_int,
                    *mut c_void,
                    *mut c_void,
                    *mut c_void,
                    *mut c_void,
                ) -> *mut c_void;
                type GetDC = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
                type ChoosePixelFormat =
                    unsafe extern "system" fn(*mut c_void, *const PixelFormatDescriptor) -> c_int;
                type SetPixelFormat = unsafe extern "system" fn(
                    *mut c_void,
                    c_int,
                    *const PixelFormatDescriptor,
                ) -> c_int;
                type WglCreateContext = unsafe extern "system" fn(*mut c_void) -> *mut c_void;
                type WglDeleteContext = unsafe extern "system" fn(*mut c_void) -> c_int;
                type WglMakeCurrent = unsafe extern "system" fn(*mut c_void, *mut c_void) -> c_int;
                type WglGetProcAddress = unsafe extern "system" fn(*const c_char) -> *const c_void;
                type WglCreateContextAttribsArb = unsafe extern "system" fn(
                    *mut c_void,
                    *mut c_void,
                    *const c_int,
                )
                    -> *mut c_void;

                let create_window: CreateWindowExA =
                    *user32.get(b"CreateWindowExA\0").expect("CreateWindowExA");
                let get_dc: GetDC = *user32.get(b"GetDC\0").expect("GetDC");
                let choose_pf: ChoosePixelFormat = *gdi32
                    .get(b"ChoosePixelFormat\0")
                    .expect("ChoosePixelFormat");
                let set_pf: SetPixelFormat =
                    *gdi32.get(b"SetPixelFormat\0").expect("SetPixelFormat");
                let wgl_create: WglCreateContext = *opengl32
                    .get(b"wglCreateContext\0")
                    .expect("wglCreateContext");
                let wgl_delete: WglDeleteContext = *opengl32
                    .get(b"wglDeleteContext\0")
                    .expect("wglDeleteContext");
                let wgl_make_current: WglMakeCurrent =
                    *opengl32.get(b"wglMakeCurrent\0").expect("wglMakeCurrent");
                let wgl_get_proc: WglGetProcAddress = *opengl32
                    .get(b"wglGetProcAddress\0")
                    .expect("wglGetProcAddress");

                let class_name = CString::new("STATIC").expect("static");
                let title = CString::new("gl_sub_test").expect("title");
                let hwnd = create_window(
                    0,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    0,
                    0,
                    0,
                    64,
                    64,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                assert!(!hwnd.is_null(), "CreateWindowExA failed");
                let hdc = get_dc(hwnd);
                assert!(!hdc.is_null(), "GetDC failed");

                let pfd = PixelFormatDescriptor {
                    n_size: u16::try_from(std::mem::size_of::<PixelFormatDescriptor>())
                        .expect("pfd size"),
                    n_version: 1,
                    dw_flags: 0x0000_0004 | 0x0000_0020 | 0x0000_0001,
                    i_pixel_type: 0,
                    c_color_bits: 32,
                    c_red_bits: 0,
                    c_red_shift: 0,
                    c_green_bits: 0,
                    c_green_shift: 0,
                    c_blue_bits: 0,
                    c_blue_shift: 0,
                    c_alpha_bits: 8,
                    c_alpha_shift: 0,
                    c_accum_bits: 0,
                    c_accum_red_bits: 0,
                    c_accum_green_bits: 0,
                    c_accum_blue_bits: 0,
                    c_accum_alpha_bits: 0,
                    c_depth_bits: 24,
                    c_stencil_bits: 8,
                    c_aux_buffers: 0,
                    i_layer_type: 0,
                    b_reserved: 0,
                    dw_layer_mask: 0,
                    dw_visible_mask: 0,
                    dw_damage_mask: 0,
                };
                let pf = choose_pf(hdc, &pfd);
                assert_ne!(pf, 0, "ChoosePixelFormat failed");
                assert_ne!(set_pf(hdc, pf, &pfd), 0, "SetPixelFormat failed");
                let dummy = wgl_create(hdc);
                assert!(!dummy.is_null(), "wglCreateContext failed");
                assert_ne!(wgl_make_current(hdc, dummy), 0, "wglMakeCurrent failed");

                let sym = CString::new("wglCreateContextAttribsARB").expect("sym");
                let raw = wgl_get_proc(sym.as_ptr());
                assert!(!raw.is_null(), "wglCreateContextAttribsARB unavailable");
                let create_attribs =
                    std::mem::transmute::<*const c_void, WglCreateContextAttribsArb>(raw);
                const WGL_CONTEXT_MAJOR_VERSION_ARB: c_int = 0x2091;
                const WGL_CONTEXT_MINOR_VERSION_ARB: c_int = 0x2092;
                const WGL_CONTEXT_PROFILE_MASK_ARB: c_int = 0x9126;
                const WGL_CONTEXT_ES2_PROFILE_BIT_EXT: c_int = 0x0004;
                let attribs = [
                    WGL_CONTEXT_MAJOR_VERSION_ARB,
                    3,
                    WGL_CONTEXT_MINOR_VERSION_ARB,
                    0,
                    WGL_CONTEXT_PROFILE_MASK_ARB,
                    WGL_CONTEXT_ES2_PROFILE_BIT_EXT,
                    0,
                ];
                let es_ctx = create_attribs(hdc, std::ptr::null_mut(), attribs.as_ptr());
                assert!(!es_ctx.is_null(), "wglCreateContextAttribsARB(ES) failed");
                assert_ne!(
                    wgl_make_current(hdc, es_ctx),
                    0,
                    "wglMakeCurrent(ES) failed"
                );
                let _ = wgl_delete(dummy);

                Self {
                    _user32: user32,
                    _gdi32: gdi32,
                    opengl32,
                    wgl_get_proc,
                }
            }
        }

        fn proc_address(&self, name: &str) -> *const c_void {
            let Ok(c_name) = CString::new(name) else {
                return std::ptr::null();
            };
            unsafe {
                let p = (self.wgl_get_proc)(c_name.as_ptr());
                let addr = p as usize;
                if addr > 3 && addr != usize::MAX {
                    return p;
                }
                self.opengl32
                    .get::<*const c_void>(c_name.as_bytes_with_nul())
                    .map_or(std::ptr::null(), |sym| *sym)
            }
        }

        fn read_texture_rgba(&self, texture: u32, width: u32, height: u32) -> Vec<u8> {
            unsafe {
                let gl = glow::Context::from_loader_function(|n| self.proc_address(n));
                let fbo = gl.create_framebuffer().expect("create read fbo");
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                gl.framebuffer_texture_2d(
                    glow::FRAMEBUFFER,
                    glow::COLOR_ATTACHMENT0,
                    glow::TEXTURE_2D,
                    NonZeroU32::new(texture).map(glow::NativeTexture),
                    0,
                );
                let mut buf = vec![0_u8; (width * height * 4) as usize];
                gl.read_pixels(
                    0,
                    0,
                    width as c_int,
                    height as c_int,
                    glow::RGBA as c_uint,
                    glow::UNSIGNED_BYTE as c_uint,
                    glow::PixelPackData::Slice(Some(&mut buf)),
                );
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                gl.delete_framebuffer(fbo);
                buf
            }
        }
    }

    fn count_bright_pixels(rgba: &[u8]) -> usize {
        rgba.as_chunks::<4>()
            .0
            .iter()
            .filter(|px| px[0] > 30 || px[1] > 30 || px[2] > 30)
            .count()
    }

    fn wait_for_subtitle_after_pre_frames(
        player: &MpvPlayer,
        target: &dyn player_api::GlVideoTarget,
        min_pre_frames: u32,
    ) -> i64 {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut pre_frames = 0_u32;
        loop {
            if matches!(target.render(640, 360), Ok(Some(pic)) if pic.fresh) {
                pre_frames += 1;
            }
            if pre_frames >= min_pre_frames {
                if let Some(track) = player
                    .tracks()
                    .into_iter()
                    .find(|t| t.kind == TrackKind::Subtitle)
                {
                    return track.id;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for subtitle track"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    #[ignore = "needs libmpv (set CS_LIBMPV_PATH) and a Windows OpenGL ES driver"]
    fn gl_subtitle_render_selecting_track_after_pre_frames_draws_pixels() {
        let wgl = WglEsContext::new();
        let library = locate_libmpv().expect("libmpv located");
        let player = MpvPlayer::new(
            &library,
            &PlayerOptions {
                embedded: true,
                gl_output: true,
                ..PlayerOptions::default()
            },
        )
        .expect("player starts");

        let target = player.gl_target().expect("gl_target");
        let loader = |name: &str| wgl.proc_address(name);
        target.attach(&loader).expect("gl attach");

        let srt_path =
            std::env::temp_dir().join(format!("cs_t4_gl_sub_{}.srt", std::process::id()));
        std::fs::write(
            &srt_path,
            "1\n00:00:00,000 --> 00:00:10,000\nHELLO WORLD SUBTITLE TEST\n\n",
        )
        .expect("write srt");

        player
            .play(&PlaybackRequest {
                url: "av://lavfi:color=c=black:s=640x360:d=10:r=10".to_owned(),
                title: "gl-sub-regression".to_owned(),
                subtitles: vec![SubtitleSource {
                    url: srt_path.to_string_lossy().into_owned(),
                    language: "English".to_owned(),
                }],
                ..PlaybackRequest::default()
            })
            .expect("play");

        // Render several frames first so GPU timer queries complete and get polled before the
        // overlay shader is compiled upon subtitle selection.
        let sub_id = wait_for_subtitle_after_pre_frames(&player, target.as_ref(), 5);
        player
            .select_track(TrackKind::Subtitle, Some(sub_id))
            .expect("select subtitle track");

        let mut frames_rendered = 0_u32;
        let mut max_bright = 0_usize;
        let watch_until = Instant::now() + Duration::from_secs(4);
        while Instant::now() < watch_until {
            if let Ok(Some(pic)) = target.render(640, 360) {
                if pic.fresh {
                    frames_rendered += 1;
                    let rgba = wgl.read_texture_rgba(pic.texture, pic.width, pic.height);
                    max_bright = max_bright.max(count_bright_pixels(&rgba));
                    if max_bright > 50 {
                        break;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(30));
        }

        target.detach();
        let _ = std::fs::remove_file(&srt_path);
        assert!(frames_rendered > 0, "expected rendered video frames");
        assert!(
            max_bright > 50,
            "selected subtitle did not render pixels onto the OpenGL ES video texture \
             (frames_rendered={frames_rendered}, max_bright={max_bright})"
        );
    }
}
