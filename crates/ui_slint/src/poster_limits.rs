//! The fixed picture limits, the same on every PC (a user decision: they are not settings).
//!
//! Decoded posters are the largest memory cost (about 0.8 MB each at 360x540), so how many exist
//! at once is bounded by what the viewer can see. Change a value here only from a measurement
//! recorded in `docs/MEMORY.md`; `poster_limits_tests.rs` pins them so a change is deliberate.

/// Posters a collapsed shelf row builds and loads. The rest wait behind the expand chevron.
pub(crate) const COLLAPSED_SHELF_POSTERS: usize = 10;
/// Posters on one page of an expanded shelf.
pub(crate) const EXPANDED_PAGE_SIZE: usize = 20;

/// Bytes of decoded poster pixels kept: every picture on screen, plus released ones up to this many
/// bytes (the ones on screen are never dropped, so a screen can exceed it). A tile decoded at 240 px
/// is 0.35 MB, so this is about 190 tiles.
pub(crate) const IMAGE_STORE_BYTES: usize = 64 * 1024 * 1024;

// A collapsed row is a prefix of the first expanded page, so expanding reuses its pictures.
const _: () = assert!(COLLAPSED_SHELF_POSTERS <= EXPANDED_PAGE_SIZE);

#[cfg(test)]
#[path = "poster_limits_tests.rs"]
mod tests;
