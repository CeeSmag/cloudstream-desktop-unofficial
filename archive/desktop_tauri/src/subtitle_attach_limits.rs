//! How many external subtitle files one stream may attach, and which language ranks count.

use subtitle_store::SKIP_DOWNLOAD_RANK;

/// Hard cap on external subtitle files attached to one open stream (mpv track-list cost).
pub(crate) const MAX_EXTERNAL_SUBTITLES_PER_STREAM: usize = 12;

/// Whether another external subtitle may be attached given how many already were and its rank.
pub(crate) fn may_attach_subtitle(already_attached: usize, rank: usize) -> bool {
    already_attached < MAX_EXTERNAL_SUBTITLES_PER_STREAM && rank != SKIP_DOWNLOAD_RANK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_ranks_attach_until_the_cap() {
        assert!(may_attach_subtitle(0, 0));
        assert!(may_attach_subtitle(11, 2));
        assert!(!may_attach_subtitle(12, 0));
        assert!(!may_attach_subtitle(0, SKIP_DOWNLOAD_RANK));
    }
}
