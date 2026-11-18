//! Fetch remote poster artwork into Slint pixel buffers.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

pub(crate) type RgbaBuffer = SharedPixelBuffer<Rgba8Pixel>;

/// Poster URL → decoded RGBA buffer, shared by the UI thread and the download threads.
#[derive(Clone, Default)]
pub(crate) struct PosterImageCache {
    inner: Arc<Mutex<HashMap<String, RgbaBuffer>>>,
}

impl PosterImageCache {
    pub(crate) fn get(&self, url: &str) -> Option<RgbaBuffer> {
        self.inner.lock().ok().and_then(|map| map.get(url).cloned())
    }

    pub(crate) fn insert(&self, url: String, buffer: RgbaBuffer) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert(url, buffer);
        }
    }
}

/// Download and decode a poster URL off the UI thread.
pub(crate) fn fetch_poster_buffer(url: &str) -> Option<RgbaBuffer> {
    if url.is_empty() {
        return None;
    }
    let response = ureq::get(url).call().ok()?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .ok()?;
    let decoded = image::load_from_memory(&bytes).ok()?.into_rgba8();
    Some(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        decoded.as_raw(),
        decoded.width(),
        decoded.height(),
    ))
}

pub(crate) fn image_from_buffer(buffer: RgbaBuffer) -> Image {
    Image::from_rgba8(buffer)
}
