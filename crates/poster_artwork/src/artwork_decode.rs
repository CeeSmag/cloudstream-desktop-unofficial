//! Decoding a downloaded image into the pixels a poster tile shows.

use crate::ArtworkError;

/// Posters are shown at most this many pixels wide (tiles, hero art on a TV); wider images are
/// shrunk so each one costs well under a megabyte of memory instead of up to 25 MB.
pub const MAX_ARTWORK_WIDTH: u32 = 360;
/// Poster tiles are decoded this wide: a tile is shown about 160 px wide, and every pixel kept in
/// memory costs 4 bytes, so a tile at 240 px costs 0.35 MB instead of 0.78 MB at 360 px. A fixed
/// choice for every PC (see `docs/MEMORY.md`).
pub const TILE_ARTWORK_WIDTH: u32 = 240;

/// How wide a decoded image may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtworkSize {
    /// A poster tile in a row or grid: [`TILE_ARTWORK_WIDTH`].
    Tile,
    /// Hero and details artwork: [`MAX_ARTWORK_WIDTH`].
    Full,
}

impl ArtworkSize {
    /// The widest this size is decoded.
    pub fn max_width(self) -> u32 {
        match self {
            Self::Tile => TILE_ARTWORK_WIDTH,
            Self::Full => MAX_ARTWORK_WIDTH,
        }
    }

    /// The key a decoded image of this size is kept under: the plain address for `Full`, so the two
    /// sizes of one address never answer each other.
    pub(crate) fn cache_key(self, url: &str) -> String {
        match self {
            Self::Tile => format!("tile|{url}"),
            Self::Full => url.to_owned(),
        }
    }
}

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
    decode_artwork_to(bytes, MAX_ARTWORK_WIDTH)
}

/// Decodes `bytes` and shrinks it to `max_width` (never enlarges).
pub fn decode_artwork_to(bytes: &[u8], max_width: u32) -> Result<DecodedArtwork, ArtworkError> {
    let image =
        image::load_from_memory(bytes).map_err(|error| ArtworkError::Decode(error.to_string()))?;
    let image = if image.width() > max_width {
        let height = scaled_height(image.width(), image.height(), max_width);
        image.thumbnail_exact(max_width, height)
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
    fn decode_artwork_to_shrinks_to_the_width_asked_for() {
        let decoded = decode_artwork_to(&png(720, 1080), 240).expect("decode");
        assert_eq!((decoded.width, decoded.height), (240, 360));
        assert_eq!(decoded.byte_size(), 240 * 360 * 4);
    }

    #[test]
    fn a_tile_is_decoded_narrower_than_a_full_image() {
        assert_eq!(ArtworkSize::Tile.max_width(), TILE_ARTWORK_WIDTH);
        assert_eq!(ArtworkSize::Full.max_width(), MAX_ARTWORK_WIDTH);
        assert!(ArtworkSize::Tile.max_width() < ArtworkSize::Full.max_width());
    }

    #[test]
    fn the_two_sizes_never_share_a_cache_key() {
        let url = "https://img.test/a.jpg";
        assert_ne!(
            ArtworkSize::Tile.cache_key(url),
            ArtworkSize::Full.cache_key(url)
        );
        assert_eq!(
            ArtworkSize::Full.cache_key(url),
            url,
            "full keeps the plain address"
        );
    }

    #[test]
    fn scaled_height_never_zero() {
        assert_eq!(scaled_height(10_000, 1, 360), 1);
        assert_eq!(scaled_height(0, 100, 360), 36_000);
    }
}
