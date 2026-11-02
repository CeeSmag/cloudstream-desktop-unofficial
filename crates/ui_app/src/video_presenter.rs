use std::sync::Arc;

use app_services::PlaybackService;
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use player_api::VideoFrame;

/// Width of the tiny picture used as the blurred backdrop for glass panels.
const BLUR_WIDTH: usize = 48;
/// Source pixels are sampled on a grid this far apart when averaging, which keeps the cost low.
const BLUR_SAMPLE_STEP: usize = 6;

/// Turns the player's frames into textures: the picture itself and a tiny blurred copy.
#[derive(Default)]
pub(crate) struct VideoPresenter {
    picture: Option<TextureHandle>,
    blurred: Option<TextureHandle>,
    last_serial: u64,
    aspect: f32,
}

impl VideoPresenter {
    /// Reports the picture area to the player and uploads a new frame when one is ready.
    /// Returns true when the textures changed.
    pub(crate) fn update(
        &mut self,
        context: &Context,
        playback: &PlaybackService,
        area_px: (u32, u32),
    ) -> bool {
        playback.set_surface_size(area_px.0, area_px.1);
        let Some(frame) = playback.latest_frame(self.last_serial) else {
            return false;
        };
        self.upload(context, &frame);
        true
    }

    fn upload(&mut self, context: &Context, frame: &Arc<VideoFrame>) {
        let size = [frame.width as usize, frame.height as usize];
        let picture = ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
        match &mut self.picture {
            Some(texture) => texture.set(picture, TextureOptions::LINEAR),
            None => {
                self.picture =
                    Some(context.load_texture("video-frame", picture, TextureOptions::LINEAR))
            }
        }
        let blurred = blur_thumbnail(frame);
        match &mut self.blurred {
            Some(texture) => texture.set(blurred, TextureOptions::LINEAR),
            None => {
                self.blurred =
                    Some(context.load_texture("video-blur", blurred, TextureOptions::LINEAR))
            }
        }
        self.last_serial = frame.serial;
        self.aspect = frame.width as f32 / frame.height.max(1) as f32;
    }

    /// The latest picture.
    pub(crate) fn picture(&self) -> Option<&TextureHandle> {
        self.picture.as_ref()
    }

    /// The latest picture, shrunk so that stretching it back up looks like a gaussian blur.
    pub(crate) fn blurred(&self) -> Option<&TextureHandle> {
        self.blurred.as_ref()
    }

    /// Forgets the frames so the next video does not flash the previous one.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Averages the frame down to a tiny image. The GPU's linear filtering then smooths it when it is
/// drawn large, which is what gives the frosted-glass look.
pub(crate) fn blur_thumbnail(frame: &VideoFrame) -> ColorImage {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let thumb_width = BLUR_WIDTH.min(width.max(1));
    let thumb_height = (thumb_width * height / width.max(1)).max(2);
    let mut pixels = Vec::with_capacity(thumb_width * thumb_height);
    for ty in 0..thumb_height {
        for tx in 0..thumb_width {
            pixels.push(average_block(frame, (tx, ty), (thumb_width, thumb_height)));
        }
    }
    ColorImage::new([thumb_width, thumb_height], pixels)
}

fn average_block(frame: &VideoFrame, cell: (usize, usize), grid: (usize, usize)) -> egui::Color32 {
    let (width, height) = (frame.width as usize, frame.height as usize);
    let (x0, x1) = (
        cell.0 * width / grid.0,
        ((cell.0 + 1) * width / grid.0).max(cell.0 * width / grid.0 + 1),
    );
    let (y0, y1) = (
        cell.1 * height / grid.1,
        ((cell.1 + 1) * height / grid.1).max(cell.1 * height / grid.1 + 1),
    );
    let (mut red, mut green, mut blue, mut count) = (0u32, 0u32, 0u32, 0u32);
    for y in (y0..y1).step_by(BLUR_SAMPLE_STEP) {
        for x in (x0..x1).step_by(BLUR_SAMPLE_STEP) {
            let at = (y * width + x) * 4;
            if let Some([r, g, b, _]) = frame
                .rgba
                .get(at..at + 4)
                .and_then(|chunk| <&[u8; 4]>::try_from(chunk).ok())
            {
                red += u32::from(*r);
                green += u32::from(*g);
                blue += u32::from(*b);
                count += 1;
            }
        }
    }
    let count = count.max(1);
    egui::Color32::from_rgb(
        (red / count) as u8,
        (green / count) as u8,
        (blue / count) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [u8; 3]) -> VideoFrame {
        let rgba = (0..width * height)
            .flat_map(|_| [rgb[0], rgb[1], rgb[2], 255])
            .collect();
        VideoFrame {
            width,
            height,
            rgba,
            serial: 1,
        }
    }

    #[test]
    fn blur_thumbnail_of_a_solid_frame_is_that_colour() {
        let image = blur_thumbnail(&solid(640, 360, [200, 100, 50]));
        assert_eq!(image.size, [BLUR_WIDTH, 27]);
        assert!(image
            .pixels
            .iter()
            .all(|pixel| *pixel == egui::Color32::from_rgb(200, 100, 50)));
    }

    #[test]
    fn blur_thumbnail_keeps_left_and_right_halves_distinct() {
        let mut frame = solid(100, 50, [0, 0, 0]);
        for y in 0..50usize {
            for x in 50..100usize {
                let at = (y * 100 + x) * 4;
                frame.rgba[at..at + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        let image = blur_thumbnail(&frame);
        let row = &image.pixels[..image.size[0]];
        assert_eq!(row[0], egui::Color32::BLACK);
        assert_eq!(row[row.len() - 1], egui::Color32::WHITE);
    }

    #[test]
    fn blur_thumbnail_of_a_tiny_frame_does_not_panic() {
        let image = blur_thumbnail(&solid(2, 2, [9, 9, 9]));
        assert!(image.size[0] >= 1 && image.size[1] >= 2);
    }
}
