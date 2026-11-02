/// One decoded picture, ready to upload as a texture.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VideoFrame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes of RGBA, alpha always 255, rows without padding.
    pub rgba: Vec<u8>,
    /// Increases with every new frame so callers can tell whether they already have it.
    pub serial: u64,
}
