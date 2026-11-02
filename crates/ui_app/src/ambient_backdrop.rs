use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use egui::load::BytesPoll;
use egui::{ColorImage, Context, Id, TextureHandle, TextureOptions};

/// Width of the shrunken artwork; stretching it back up gives a soft colour wash.
const AMBIENT_WIDTH: u32 = 24;
/// Height of the shrunken artwork (posters are portrait).
const AMBIENT_HEIGHT: u32 = 36;

type Cache = Arc<Mutex<HashMap<String, Option<TextureHandle>>>>;

/// A heavily blurred copy of the artwork at `url`, once its bytes have loaded.
///
/// The first call starts loading; later calls return the texture as soon as it is ready. An
/// image that cannot be decoded is remembered as missing so it is not retried every frame.
pub(crate) fn texture_for(context: &Context, url: &str) -> Option<TextureHandle> {
    let cache: Cache = context.data_mut(|data| {
        data.get_temp_mut_or_insert_with(Id::unique("ambient-cache"), Cache::default)
            .clone()
    });
    let mut entries = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(known) = entries.get(url) {
        return known.clone();
    }
    match context.try_load_bytes(url) {
        Ok(BytesPoll::Ready { bytes, .. }) => {
            let texture = blur_image_bytes(&bytes).map(|image| {
                context.load_texture(format!("ambient:{url}"), image, TextureOptions::LINEAR)
            });
            entries.insert(url.to_owned(), texture.clone());
            texture
        }
        Ok(BytesPoll::Pending { .. }) => None,
        Err(error) => {
            tracing::debug!(%error, url, "artwork not available for ambient backdrop");
            entries.insert(url.to_owned(), None);
            None
        }
    }
}

/// Decodes an image and shrinks it to a tiny, smooth picture.
pub(crate) fn blur_image_bytes(bytes: &[u8]) -> Option<ColorImage> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let small = decoded
        .resize_exact(
            AMBIENT_WIDTH,
            AMBIENT_HEIGHT,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied(
        [AMBIENT_WIDTH as usize, AMBIENT_HEIGHT as usize],
        small.as_raw(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        let image = image::RgbImage::from_pixel(width, height, image::Rgb(rgb));
        image
            .write_to(&mut buffer, image::ImageFormat::Png)
            .expect("png encodes");
        buffer.into_inner()
    }

    #[test]
    fn blur_image_bytes_shrinks_to_the_ambient_size_and_keeps_colour() {
        let image = blur_image_bytes(&png_bytes(300, 450, [10, 120, 240])).expect("decodes");
        assert_eq!(
            image.size,
            [AMBIENT_WIDTH as usize, AMBIENT_HEIGHT as usize]
        );
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(10, 120, 240));
    }

    #[test]
    fn blur_image_bytes_rejects_garbage() {
        assert!(blur_image_bytes(b"not an image").is_none());
    }
}
