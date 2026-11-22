//! Where the video player draws its pictures.

/// The software path renders each picture into memory and the window copies it; the GPU path draws
/// into a texture of the window's OpenGL context and shows it as it is (no copy). The GPU path was
/// verified on Linux/Wayland only, so every profile starts on the software path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoRenderer {
    /// Pictures rendered into memory (works everywhere).
    Cpu,
    /// Pictures drawn straight into the window's OpenGL context (zero-copy video).
    Gpu,
}

impl VideoRenderer {
    /// The values the settings screen offers.
    pub const CHOICES: [VideoRenderer; 2] = [Self::Cpu, Self::Gpu];

    /// Whether this choice asks for zero-copy video.
    pub fn draws_into_the_window(self) -> bool {
        self == Self::Gpu
    }

    /// The text kept in the settings database.
    pub fn to_text(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }

    /// Reads [`Self::to_text`] back; anything else is not accepted.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::CHOICES
            .into_iter()
            .find(|choice| choice.to_text().eq_ignore_ascii_case(text))
    }
}

#[cfg(test)]
#[path = "video_renderer_tests.rs"]
mod tests;
