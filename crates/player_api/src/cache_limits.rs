//! How much of the playing stream the player keeps in memory.

/// The memory the player may use for the stream that plays: what it reads ahead of the playing
/// position, and what it keeps of what was already played so a jump back needs no download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheLimits {
    /// Bytes read ahead of the playing position.
    pub ahead_bytes: u64,
    /// Bytes kept behind the playing position.
    pub behind_bytes: u64,
}
