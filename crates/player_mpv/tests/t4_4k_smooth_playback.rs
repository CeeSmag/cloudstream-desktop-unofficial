//! T4: verifies that 4K playback stays smooth and low-CPU on both the fast software fallback path
//! and (on Linux with EGL) the zero-copy OpenGL path, and that a failed GL attach automatically
//! falls back to software frames instead of leaving a black screen.
//!
//! Run with:
//! `CS_LIBMPV_PATH=/usr/lib64/libmpv.so.2 CS_BENCH_VIDEO=/path/to/4k60.mp4 \
//!  cargo test -p player_mpv --release --test t4_4k_smooth_playback -- --ignored --nocapture`
#![allow(
    unsafe_code,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::print_stdout,
    clippy::too_many_lines
)]

use std::time::{Duration, Instant};

use player_api::{PlaybackRequest, PlaybackState, VideoPlayer};
use player_mpv::{locate_libmpv, MpvPlayer, PlayerOptions};

const FRAME_TICK: Duration = Duration::from_millis(16);

#[cfg(target_os = "linux")]
fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after_name = stat.rsplit_once(')').map_or("", |(_, rest)| rest);
    let ticks = |index: usize| -> f64 {
        after_name
            .split_whitespace()
            .nth(index)
            .and_then(|field| field.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

#[test]
#[ignore = "needs libmpv (set CS_LIBMPV_PATH)"]
fn failed_gl_attach_falls_back_to_software_frames_instead_of_a_black_screen() {
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
    let target = player.gl_target().expect("gl target before attach");
    let broken_loader = |_name: &str| std::ptr::null();
    assert!(
        target.attach(&broken_loader).is_err(),
        "null GL loader must fail attach"
    );
    assert!(
        player.gl_target().is_none(),
        "after GL attach fails, player must switch off gl_target so the UI reads software frames"
    );

    player
        .play(&PlaybackRequest {
            url: "av://lavfi:testsrc=duration=5:size=320x180:rate=15".to_owned(),
            title: "fallback".to_owned(),
            ..PlaybackRequest::default()
        })
        .expect("play");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut got_frame = false;
    while Instant::now() < deadline {
        if let Some(frame) = player.latest_frame(0) {
            assert!(frame.width > 0 && frame.height > 0);
            got_frame = true;
            break;
        }
        std::thread::sleep(FRAME_TICK);
    }
    assert!(
        got_frame,
        "software fallback must deliver video frames after GL attach fails"
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs libmpv (CS_LIBMPV_PATH) and a 4K 60fps video (CS_BENCH_VIDEO)"]
fn software_fallback_plays_4k_without_pegging_the_cpu() {
    let video = std::env::var("CS_BENCH_VIDEO").expect("CS_BENCH_VIDEO");
    let library = locate_libmpv().expect("libmpv located");
    let player = MpvPlayer::new(
        &library,
        &PlayerOptions {
            embedded: true,
            gl_output: false,
            ..PlayerOptions::default()
        },
    )
    .expect("player starts");
    player.set_surface_size(1280, 720);
    player.set_volume(0).expect("mute");
    player
        .play(&PlaybackRequest {
            url: video,
            title: "sw-4k".to_owned(),
            ..PlaybackRequest::default()
        })
        .expect("play");

    let mut last_serial = 0_u64;
    let wait_start = Instant::now();
    while wait_start.elapsed() < Duration::from_secs(10) {
        if let Some(frame) = player.latest_frame(last_serial) {
            last_serial = frame.serial;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(last_serial > 0, "first frame never arrived");

    let cpu_before = cpu_seconds();
    let start = Instant::now();
    let mut delivered = 0_u32;
    while start.elapsed() < Duration::from_secs(5) {
        if let Some(frame) = player.latest_frame(last_serial) {
            last_serial = frame.serial;
            delivered += 1;
            std::hint::black_box(frame.rgba.clone());
        }
        std::thread::sleep(FRAME_TICK);
    }
    let wall = start.elapsed().as_secs_f64();
    let cpu_pct = ((cpu_seconds() - cpu_before) / wall) * 100.0;
    let ui_fps = f64::from(delivered) / wall;
    println!("SW-4K wall={wall:.2}s ui_fps={ui_fps:.1} cpu_pct={cpu_pct:.0}%");
    assert!(
        matches!(
            player.status().state,
            PlaybackState::Playing | PlaybackState::Paused | PlaybackState::Ended
        ),
        "unexpected state"
    );
    assert!(ui_fps >= 50.0, "ui_fps {ui_fps:.1} < 50.0");
    assert!(
        cpu_pct <= 180.0,
        "software 4K CPU {cpu_pct:.0}% exceeded 180% budget (was ~465% before sws fast-bilinear)"
    );
}

#[cfg(target_os = "linux")]
type EglGetProcAddr = unsafe extern "C" fn(*const std::ffi::c_char) -> *const std::ffi::c_void;

#[cfg(target_os = "linux")]
fn create_headless_egl() -> (libloading::Library, EglGetProcAddr) {
    use std::ffi::{c_int, c_uint, c_void};

    let egl = unsafe { libloading::Library::new("libEGL.so.1").expect("libEGL.so.1") };
    type GetPlatformDisplay =
        unsafe extern "C" fn(c_uint, *mut c_void, *const isize) -> *mut c_void;
    type Initialize = unsafe extern "C" fn(*mut c_void, *mut c_int, *mut c_int) -> c_uint;
    type BindApi = unsafe extern "C" fn(c_uint) -> c_uint;
    type ChooseConfig = unsafe extern "C" fn(
        *mut c_void,
        *const c_int,
        *mut *mut c_void,
        c_int,
        *mut c_int,
    ) -> c_uint;
    type CreateContext =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *const c_int) -> *mut c_void;
    type MakeCurrent =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, *mut c_void) -> c_uint;

    const EGL_PLATFORM_SURFACELESS_MESA: c_uint = 0x31DD;
    const EGL_OPENGL_API: c_uint = 0x30A2;
    const EGL_RENDERABLE_TYPE: c_int = 0x3040;
    const EGL_OPENGL_BIT: c_int = 0x0008;
    const EGL_NONE: c_int = 0x3038;

    let get_proc: EglGetProcAddr = unsafe {
        let get_plat_dpy: GetPlatformDisplay = *egl.get(b"eglGetPlatformDisplay\0").expect("sym");
        let init: Initialize = *egl.get(b"eglInitialize\0").expect("sym");
        let bind_api: BindApi = *egl.get(b"eglBindAPI\0").expect("sym");
        let choose_cfg: ChooseConfig = *egl.get(b"eglChooseConfig\0").expect("sym");
        let create_ctx: CreateContext = *egl.get(b"eglCreateContext\0").expect("sym");
        let make_current: MakeCurrent = *egl.get(b"eglMakeCurrent\0").expect("sym");
        let get_proc: EglGetProcAddr = *egl.get(b"eglGetProcAddress\0").expect("sym");

        let dpy = get_plat_dpy(
            EGL_PLATFORM_SURFACELESS_MESA,
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        assert!(!dpy.is_null(), "eglGetPlatformDisplay failed");
        let (mut maj, mut min) = (0, 0);
        assert_ne!(init(dpy, &mut maj, &mut min), 0, "eglInitialize failed");
        assert_ne!(bind_api(EGL_OPENGL_API), 0, "eglBindAPI failed");
        let cfg_attribs = [EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT, EGL_NONE];
        let mut cfg = std::ptr::null_mut();
        let mut n = 0;
        assert_ne!(
            choose_cfg(dpy, cfg_attribs.as_ptr(), &mut cfg, 1, &mut n),
            0
        );
        let ctx_attribs = [EGL_NONE];
        let ctx = create_ctx(
            dpy,
            if n > 0 { cfg } else { std::ptr::null_mut() },
            std::ptr::null_mut(),
            ctx_attribs.as_ptr(),
        );
        assert!(!ctx.is_null(), "eglCreateContext failed");
        assert_ne!(
            make_current(dpy, std::ptr::null_mut(), std::ptr::null_mut(), ctx),
            0,
            "eglMakeCurrent failed"
        );
        get_proc
    };
    (egl, get_proc)
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "needs libmpv (CS_LIBMPV_PATH), EGL, and a 4K 60fps video (CS_BENCH_VIDEO)"]
fn gpu_zero_copy_plays_4k_60fps_smoothly_with_event_driven_wakes() {
    use std::ffi::{c_void, CString};
    use std::sync::{Arc, Condvar, Mutex};

    let video = std::env::var("CS_BENCH_VIDEO").expect("CS_BENCH_VIDEO");
    let (_egl, get_proc) = create_headless_egl();

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
    let loader = |name: &str| -> *const c_void {
        CString::new(name).map_or(std::ptr::null(), |c| unsafe { get_proc(c.as_ptr()) })
    };
    target.attach(&loader).expect("gl attach");

    let wake_pair = Arc::new((Mutex::new(false), Condvar::new()));
    let wake_sender = Arc::clone(&wake_pair);
    target.set_frame_wake(Box::new(move || {
        let (lock, cvar) = &*wake_sender;
        if let Ok(mut ready) = lock.lock() {
            *ready = true;
            cvar.notify_one();
        }
    }));

    player.set_volume(0).expect("mute");
    player
        .play(&PlaybackRequest {
            url: video,
            title: "gl-4k".to_owned(),
            ..PlaybackRequest::default()
        })
        .expect("play");

    let wait_frame = |timeout: Duration| {
        let (lock, cvar) = &*wake_pair;
        let guard = lock.lock().expect("lock");
        let (mut guard, _) = cvar
            .wait_timeout_while(guard, timeout, |ready| !*ready)
            .expect("condvar");
        *guard = false;
    };

    let warm = Instant::now();
    while warm.elapsed() < Duration::from_secs(10) {
        wait_frame(Duration::from_millis(50));
        if matches!(target.render(1920, 1080), Ok(Some(_))) {
            break;
        }
    }

    let cpu_before = cpu_seconds();
    let start = Instant::now();
    let mut rendered = 0_u32;
    let mut render_us_sum: u128 = 0;
    while start.elapsed() < Duration::from_secs(5) {
        wait_frame(Duration::from_millis(50));
        let t0 = Instant::now();
        if let Ok(Some(pic)) = target.render(1920, 1080) {
            if pic.fresh {
                rendered += 1;
                render_us_sum += t0.elapsed().as_micros();
            }
        }
    }

    let wall = start.elapsed().as_secs_f64();
    let cpu_pct = ((cpu_seconds() - cpu_before) / wall) * 100.0;
    let fps = f64::from(rendered) / wall;
    let render_avg_ms = render_us_sum as f64 / f64::from(rendered.max(1)) / 1000.0;
    println!(
        "GL-4K wall={wall:.2}s size=1920x1080 fps={fps:.1} render_avg={render_avg_ms:.2}ms cpu_pct={cpu_pct:.0}%"
    );
    target.detach();
    assert!(fps >= 55.0, "GL 4K fps {fps:.1} < 55.0");
    assert!(
        cpu_pct <= 35.0,
        "GL 4K zero-copy CPU {cpu_pct:.0}% > 35% (was 112% before DRM interop / 465% on SW)"
    );
}
