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

    fn publish(&self, frame: VideoFrame) {
        *self.slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(frame));
    }
}

fn pack(width: u32, height: u32) -> u64 {
    (u64::from(width) << 32) | u64::from(height)
}

/// Scales a requested surface down to the render cap and keeps both sides even and not tiny.
pub(crate) fn fit_surface(width: u32, height: u32) -> (u32, u32) {
    let (mut width, mut height) = (width.max(MIN_SIDE), height.max(MIN_SIDE));
    if width > MAX_RENDER_WIDTH {
        height = (u64::from(height) * u64::from(MAX_RENDER_WIDTH) / u64::from(width)) as u32;
        width = MAX_RENDER_WIDTH;
    }
    (width & !1, height.max(MIN_SIDE) & !1)
}

/// Bytes per row for `width` pixels at 4 bytes each, rounded up to the alignment.
pub(crate) fn aligned_stride(width: u32) -> usize {
    (width as usize * 4).div_ceil(STRIDE_ALIGNMENT) * STRIDE_ALIGNMENT
}

/// Converts mpv's `rgb0` rows (padded to `stride`) into tightly packed RGBA with alpha 255.
pub(crate) fn to_rgba(source: &[u8], size: (u32, u32), stride: usize) -> Vec<u8> {
    let (width, height) = (size.0 as usize, size.1 as usize);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in source.chunks(stride).take(height) {
        let (pixels, _) = row.as_chunks::<4>();
        for [red, green, blue, _] in pixels.iter().take(width) {
            rgba.extend_from_slice(&[*red, *green, *blue, 255]);
        }
    }
    rgba
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
    let mut buffer: Vec<u8> = Vec::new();
    let mut serial: u64 = 0;
    while !stop.load(Ordering::Relaxed) {
        signal.wait(WAIT);
        if !context.has_new_frame() {
            continue;
        }
        let size = shared.size();
        let stride = aligned_stride(size.0);
        buffer.resize(stride * size.1 as usize, 0);
        match context.render(size, stride, &mut buffer) {
            Ok(()) => {
                serial += 1;
                shared.publish(VideoFrame {
                    width: size.0,
                    height: size.1,
                    rgba: to_rgba(&buffer, size, stride),
                    serial,
                });
            }
            Err(error) => tracing::debug!(%error, "frame render failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_surface_caps_width_keeps_aspect_and_evens_sides() {
        assert_eq!(fit_surface(2560, 1440), (1280, 720));
        assert_eq!(fit_surface(801, 451), (800, 450));
        assert_eq!(fit_surface(10, 10), (64, 64));
    }

    #[test]
    fn aligned_stride_rounds_rows_up_to_64_bytes() {
        assert_eq!(aligned_stride(16), 64);
        assert_eq!(aligned_stride(17), 128);
    }

    #[test]
    fn to_rgba_drops_padding_and_forces_opaque_alpha() {
        // 2x2 image, stride 16: each row = 2 pixels (8 bytes) + 8 bytes padding.
        let mut source = vec![0u8; 32];
        source[..4].copy_from_slice(&[1, 2, 3, 0]);
        source[4..8].copy_from_slice(&[4, 5, 6, 0]);
        source[16..20].copy_from_slice(&[7, 8, 9, 0]);
        source[20..24].copy_from_slice(&[10, 11, 12, 0]);
        let rgba = to_rgba(&source, (2, 2), 16);
        assert_eq!(
            rgba,
            [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
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
}
