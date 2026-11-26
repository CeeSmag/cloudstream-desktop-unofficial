//! Where the video player draws (Settings > General > "Video output").

use memory_budget::VideoRenderer;

use crate::{ServiceError, SettingsService};

pub(crate) const VIDEO_RENDERER_KEY: &str = "video_renderer";

impl SettingsService {
    /// Where the player draws its pictures: what the viewer chose, else the profile's value (the
    /// GPU path, with automatic software fallback). Read when the first video starts, so a change
    /// applies after a restart.
    pub fn video_renderer(&self) -> Result<VideoRenderer, ServiceError> {
        Ok(self.performance_budget()?.video_renderer)
    }

    /// Remembers the viewer's choice of video output.
    pub fn set_video_renderer(&self, renderer: VideoRenderer) -> Result<(), ServiceError> {
        self.store_value(VIDEO_RENDERER_KEY, renderer.to_text())
    }

    /// The viewer's own choice, if one is stored and readable.
    pub(crate) fn stored_video_renderer(&self) -> Result<Option<VideoRenderer>, ServiceError> {
        Ok(self
            .stored(VIDEO_RENDERER_KEY)?
            .as_deref()
            .and_then(VideoRenderer::parse))
    }
}
