//! Where the video player draws its pictures.

/// The GPU path draws into a texture of the window's OpenGL context and shows it as it is (no
/// copy), falling back automatically to the software path when the window has no OpenGL context or
/// attaching fails. Every profile starts on GPU so 4K playback stays smooth on low-end PCs.
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
