//! Decoding a downloaded image into the pixels a poster tile shows.

use crate::ArtworkError;

/// Posters are shown at most this many pixels wide (tiles, hero art on a TV); wider images are
/// shrunk so each one costs well under a megabyte of memory instead of up to 25 MB.
pub const MAX_ARTWORK_WIDTH: u32 = 360;

/// A decoded poster, 8-bit RGBA, row after row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedArtwork {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

impl DecodedArtwork {
    /// Memory the pixels take.
    pub fn byte_size(&self) -> usize {
        self.rgba.len()
    }
}

/// Decodes `bytes` (JPEG, PNG, WebP or GIF) and shrinks it to [`MAX_ARTWORK_WIDTH`].
pub fn decode_artwork(bytes: &[u8]) -> Result<DecodedArtwork, ArtworkError> {
    let image =
        image::load_from_memory(bytes).map_err(|error| ArtworkError::Decode(error.to_string()))?;
    let image = if image.width() > MAX_ARTWORK_WIDTH {
        let height = scaled_height(image.width(), image.height(), MAX_ARTWORK_WIDTH);
        image.thumbnail_exact(MAX_ARTWORK_WIDTH, height)
    } else {
        image
    };
    let rgba = image.into_rgba8();
    Ok(DecodedArtwork {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// The height that keeps the aspect ratio at `target_width`, at least 1.
fn scaled_height(width: u32, height: u32, target_width: u32) -> u32 {
    let scaled = u64::from(height) * u64::from(target_width) / u64::from(width.max(1));
    u32::try_from(scaled).unwrap_or(u32::MAX).max(1)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("encode");
        bytes
    }

    #[test]
    fn decode_artwork_wide_image_is_shrunk_keeping_aspect() {
        let decoded = decode_artwork(&png(720, 1080)).expect("decode");
        assert_eq!((decoded.width, decoded.height), (360, 540));
        assert_eq!(decoded.byte_size(), 360 * 540 * 4);
    }

    #[test]
    fn decode_artwork_small_image_is_kept() {
        let decoded = decode_artwork(&png(100, 150)).expect("decode");
        assert_eq!((decoded.width, decoded.height), (100, 150));
        assert_eq!(decoded.rgba.get(..4), Some(&[10, 20, 30, 255][..]));
    }

    #[test]
    fn decode_artwork_html_error_page_is_a_decode_error() {
        let page = b"<html><body>Forbidden</body></html>";
        assert!(matches!(decode_artwork(page), Err(ArtworkError::Decode(_))));
    }

    #[test]
    fn decode_artwork_truncated_image_is_a_decode_error() {
        let whole = png(400, 600);
        let truncated = whole.get(..whole.len() / 2).expect("half");
        assert!(matches!(
            decode_artwork(truncated),
            Err(ArtworkError::Decode(_))
        ));
    }

    #[test]
    fn scaled_height_never_zero() {
        assert_eq!(scaled_height(10_000, 1, 360), 1);
        assert_eq!(scaled_height(0, 100, 360), 36_000);
    }
}
