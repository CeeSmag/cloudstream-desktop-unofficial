use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use player_api::{PlayerError, VideoFrame};

use crate::mpv_render_api::{FrameSignal, MpvRenderContext};

/// How long the render thread sleeps between checks of its stop flag.
const WAIT: Duration = Duration::from_millis(50);
/// Largest picture width rendered on the CPU; the GPU scales it up to the window.
pub(crate) const MAX_RENDER_WIDTH: u32 = 1280;
/// Largest picture height rendered on the CPU. Only the width was capped before, so a tall,
/// narrow window (1280 x 3000) cost 15 MB a frame; the GPU scales it up either way.
pub(crate) const MAX_RENDER_HEIGHT: u32 = 1080;
/// Rows and columns are kept at least this large.
const MIN_SIDE: u32 = 64;
/// mpv is fastest when each row starts on a 64-byte boundary.
const STRIDE_ALIGNMENT: usize = 64;

/// State shared between the render thread and the UI thread.
pub(crate) struct FrameShared {
    slot: Mutex<Option<Arc<VideoFrame>>>,
    size: AtomicU64,
}

impl FrameShared {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        Self {
            slot: Mutex::new(None),
            size: AtomicU64::new(pack(width, height)),
        }
    }

    /// Sets the picture area; the size is capped to what the CPU renderer can keep up with.
    pub(crate) fn set_size(&self, width: u32, height: u32) {
        let (width, height) = fit_surface(width, height);
        self.size.store(pack(width, height), Ordering::Relaxed);
    }

    fn size(&self) -> (u32, u32) {
        let packed = self.size.load(Ordering::Relaxed);
        ((packed >> 32) as u32, (packed & 0xffff_ffff) as u32)
    }

    pub(crate) fn latest(&self, last_seen_serial: u64) -> Option<Arc<VideoFrame>> {
        let slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        slot.as_ref()
            .filter(|frame| frame.serial != last_seen_serial)
            .cloned()
    }

    /// Drops the last picture so a finished stream does not stay on screen.
    pub(crate) fn clear(&self) {
        *self.slot.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Shows `frame` as the newest picture. Returns the pixel buffer of the picture it replaced when
    /// nobody else holds that picture, so the render thread can fill it again instead of
    /// allocating a new one for every frame (about 3.5 MiB at 1280x720, 30-60 times a second).
    pub(crate) fn publish(&self, frame: VideoFrame) -> Option<Vec<u8>> {
        let replaced = self
            .slot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(Arc::new(frame));
        // Exclusive only when the UI has dropped its copy: then no reader can see this buffer
        // change. Otherwise it is simply freed when the reader is done.
        replaced
            .and_then(|old| Arc::try_unwrap(old).ok())
            .map(|old| old.rgba)
    }
}

fn pack(width: u32, height: u32) -> u64 {
    (u64::from(width) << 32) | u64::from(height)
}

/// Scales a requested surface down to the render cap, keeps it from being tiny, and rounds the
/// width to a multiple of 16 so a row is exactly `width * 4` bytes and 64-byte aligned: mpv then
/// writes the picture in its final layout, with no padding to strip and no second copy.
pub(crate) fn fit_surface(width: u32, height: u32) -> (u32, u32) {
    let (mut width, mut height) = (width.max(MIN_SIDE), height.max(MIN_SIDE));
    if width > MAX_RENDER_WIDTH {
        height = (u64::from(height) * u64::from(MAX_RENDER_WIDTH) / u64::from(width)) as u32;
        width = MAX_RENDER_WIDTH;
    }
    if height > MAX_RENDER_HEIGHT {
        width = (u64::from(width) * u64::from(MAX_RENDER_HEIGHT) / u64::from(height)) as u32;
        height = MAX_RENDER_HEIGHT;
    }
    (round_to_sixteen(width), height.max(MIN_SIDE) & !1)
}

/// The multiple of 16 nearest `width` (never below the minimum side).
fn round_to_sixteen(width: u32) -> u32 {
    ((width + 8) & !15).max(MIN_SIDE)
}

/// Bytes per row for `width` pixels at 4 bytes each, rounded up to the alignment.
pub(crate) fn aligned_stride(width: u32) -> usize {
    (width as usize * 4).div_ceil(STRIDE_ALIGNMENT) * STRIDE_ALIGNMENT
}

/// mpv's `rgb0` format leaves the fourth byte of each pixel undefined; the UI wants opaque RGBA.
fn make_opaque(pixels: &mut [u8]) {
    let (chunks, _) = pixels.as_chunks_mut::<4>();
    for [_, _, _, alpha] in chunks {
        *alpha = 255;
    }
}

/// The background thread that turns mpv's frame notifications into RGBA pictures.
pub(crate) struct FrameRenderer {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FrameRenderer {
    pub(crate) fn start(
        context: MpvRenderContext,
        signal: Arc<FrameSignal>,
        shared: Arc<FrameShared>,
    ) -> Result<Self, PlayerError> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("mpv-frames".to_owned())
            .spawn(move || run(&context, &signal, &shared, &thread_stop))
            .map_err(|error| {
                PlayerError::Unavailable(format!("cannot start render thread: {error}"))
            })?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for FrameRenderer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::warn!("mpv render thread panicked");
            }
        }
    }
}

fn run(context: &MpvRenderContext, signal: &FrameSignal, shared: &FrameShared, stop: &AtomicBool) {
    // The buffer of the previous picture, when nobody holds it any more: filled again, so the
    // steady state allocates nothing per frame.
    let mut spare: Option<Vec<u8>> = None;
    let mut serial: u64 = 0;
    while !stop.load(Ordering::Relaxed) {
        signal.wait(WAIT);
        if !context.has_new_frame() {
            continue;
        }
        let size = shared.size();
        let stride = aligned_stride(size.0);
        let mut buffer = spare.take().unwrap_or_default();
        buffer.resize(stride * size.1 as usize, 0);
        match context.render(size, stride, &mut buffer) {
            Ok(()) => {
                make_opaque(&mut buffer);
                serial += 1;
                spare = shared.publish(VideoFrame {
                    width: size.0,
                    height: size.1,
                    rgba: buffer,
                    serial,
                });
            }
            Err(error) => {
                tracing::debug!(%error, "frame render failed");
                spare = Some(buffer);
            }
        }
    }
}

/// Size used for the first frames, before the UI reports its real picture area.
pub(crate) const INITIAL_SURFACE: (u32, u32) = (1280, 720);

/// Whether frames are rendered into memory for the app window rather than in mpv's own window.
pub(crate) fn wants_embedding(
    api: &crate::mpv_api::MpvApi,
    options: &crate::player_configuration::PlayerOptions,
) -> bool {
    let embedded = options.embedded
        && !options.headless
        && options.wid.is_none()
        && api.render_api().is_some();
    if options.embedded && !embedded && !options.headless {
        tracing::warn!("this libmpv has no render API; using mpv's own window");
    }
    embedded
}

/// Starts frame rendering; on failure falls back to mpv's own window.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn start_embedding(
    handle: &Arc<crate::mpv_api::MpvHandle>,
) -> (Option<FrameRenderer>, Option<Arc<FrameShared>>) {
    let shared = Arc::new(FrameShared::new(INITIAL_SURFACE.0, INITIAL_SURFACE.1));
    match start_embedding_into(handle, Arc::clone(&shared)) {
        Some(renderer) => (Some(renderer), Some(shared)),
        None => (None, None),
    }
}

/// Starts software frame rendering into an existing `shared` slot (used both at start-up and when
/// an OpenGL render context fails to attach at runtime). Falls back to mpv's own window on failure.
#[tracing::instrument(level = "debug", skip_all)]
pub(crate) fn start_embedding_into(
    handle: &Arc<crate::mpv_api::MpvHandle>,
    shared: Arc<FrameShared>,
) -> Option<FrameRenderer> {
    let signal = Arc::new(FrameSignal::default());
    let started = MpvRenderContext::create(Arc::clone(handle), Arc::clone(&signal))
        .and_then(|context| FrameRenderer::start(context, signal, shared));
    match started {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            tracing::warn!(%error, "embedded rendering failed; using mpv's own window");
            if let Err(error) = handle.set_property("vo", "gpu") {
                tracing::warn!(%error, "could not switch to a window");
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn fit_surface_caps_width_keeps_aspect_and_evens_sides() {
        assert_eq!(fit_surface(2560, 1440), (1280, 720));
        assert_eq!(fit_surface(1000, 500), (1008, 500));
        assert_eq!(fit_surface(10, 10), (64, 64));
    }

    #[test]
    fn a_tall_narrow_window_is_capped_in_height_too_keeping_its_shape() {
        let (width, height) = fit_surface(1280, 3000);
        assert_eq!(height, MAX_RENDER_HEIGHT);
        assert_eq!(width, 464, "1280 x 1080 / 3000, to a multiple of 16");
        assert!(u64::from(width) * u64::from(height) * 4 < 2 * 1024 * 1024);
    }

    #[test]
    fn fit_surface_width_is_a_multiple_of_16_so_rows_need_no_padding() {
        for width in [64, 100, 333, 640, 801, 1279, 1280, 1920, 4000] {
            let (fitted, _) = fit_surface(width, 600);
            assert_eq!(fitted % 16, 0, "width {width} became {fitted}");
            assert_eq!(
                aligned_stride(fitted),
                fitted as usize * 4,
                "rows are tight"
            );
        }
    }

    #[test]
    fn make_opaque_sets_every_alpha_byte_and_keeps_the_colors() {
        let mut pixels = vec![1, 2, 3, 0, 4, 5, 6, 7, 8, 9, 10, 0];
        make_opaque(&mut pixels);
        assert_eq!(pixels, [1, 2, 3, 255, 4, 5, 6, 255, 8, 9, 10, 255]);
    }

    #[test]
    fn publish_hands_back_the_buffer_of_the_frame_it_replaces_when_nobody_holds_it() {
        let shared = FrameShared::new(64, 64);
        assert!(
            shared.publish(frame(1, 16)).is_none(),
            "nothing to hand back yet"
        );
        let recycled = shared
            .publish(frame(2, 16))
            .expect("the first frame's buffer");
        assert_eq!(recycled.len(), 16);
    }

    #[test]
    fn a_buffer_a_consumer_still_holds_is_not_handed_back() {
        let shared = FrameShared::new(64, 64);
        shared.publish(frame(1, 16));
        let held = shared.latest(0).expect("a frame");
        assert!(
            shared.publish(frame(2, 16)).is_none(),
            "the consumer still reads frame 1"
        );
        assert_eq!(held.serial, 1);
        assert!(
            held.rgba.iter().all(|byte| *byte == 1),
            "frame 1 was left alone"
        );
    }

    #[test]
    fn shared_latest_skips_the_frame_the_caller_already_has() {
        let shared = FrameShared::new(64, 64);
        shared.publish(VideoFrame {
            serial: 3,
            ..VideoFrame::default()
        });
        assert!(shared.latest(3).is_none());
        assert_eq!(shared.latest(2).map(|frame| frame.serial), Some(3));
    }

    /// A frame whose every byte is `serial % 251`: a torn or reused frame shows as mixed bytes.
    fn frame(serial: u64, len: usize) -> VideoFrame {
        VideoFrame {
            width: 2,
            height: 2,
            serial,
            rgba: vec![(serial % 251) as u8; len],
        }
    }

    /// How many frames the stress producer publishes; the last one stays published.
    const FINAL_SERIAL: u64 = 6_000;

    /// Publishes every frame up to [`FINAL_SERIAL`] the way the render thread does, recycling the
    /// buffer handed back.
    fn produce(shared: &FrameShared) {
        let mut spare: Option<Vec<u8>> = None;
        for serial in 1..=FINAL_SERIAL {
            let mut buffer = spare.take().unwrap_or_default();
            buffer.clear();
            buffer.resize(4096, (serial % 251) as u8);
            spare = shared.publish(VideoFrame {
                width: 32,
                height: 32,
                serial,
                rgba: buffer,
            });
        }
    }

    /// Reads frames until it has seen the final one (it stays published, so every reader gets
    /// there however late it was scheduled); returns the last serial it saw. Every frame must keep
    /// its one value while it is read, and serials only grow. The deadline only guards a hang.
    fn consume(shared: &FrameShared) -> u64 {
        let mut last = 0_u64;
        let deadline = Instant::now() + Duration::from_secs(20);
        while last < FINAL_SERIAL && Instant::now() < deadline {
            let Some(frame) = shared.latest(last) else {
                continue;
            };
            assert!(frame.serial > last, "serials must only grow");
            let first = frame.rgba[0];
            std::thread::yield_now();
            assert!(
                frame.rgba.iter().all(|byte| *byte == first),
                "frame {} changed or mixed under its reader",
                frame.serial
            );
            assert_eq!(first, (frame.serial % 251) as u8);
            last = frame.serial;
        }
        last
    }

    /// A consumer must never see a recycled frame change under it or mix two frames, and every
    /// consumer ends on the final frame whatever the scheduling (the test once ended a consumer
    /// when the producer finished, so a consumer scheduled late on a loaded PC saw nothing).
    #[test]
    fn stress_recycled_buffers_are_never_seen_changing_by_a_consumer() {
        let shared = Arc::new(FrameShared::new(64, 64));
        let producer = {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || produce(&shared))
        };
        let consumers: Vec<_> = (0..3)
            .map(|_| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || consume(&shared))
            })
            .collect();
        producer.join().expect("producer");
        for consumer in consumers {
            assert_eq!(
                consumer.join().expect("consumer"),
                FINAL_SERIAL,
                "a consumer never reached the final frame"
            );
        }
    }
}
