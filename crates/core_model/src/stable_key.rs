//! Stable names for files kept on disk (cached subtitles, saved sources).

const PRIME: u64 = 0x0000_0100_0000_01b3;
const BASIS_A: u64 = 0xcbf2_9ce4_8422_2325;
const BASIS_B: u64 = 0x8422_2325_cbf2_9ce4;

/// 32 hex characters that identify `text`. The same text always gives the same name, on every run
/// and every Rust version, so a file saved yesterday is found again today.
pub fn stable_key(text: &str) -> String {
    let (mut first, mut second) = (BASIS_A, BASIS_B);
    for byte in text.bytes() {
        first = (first ^ u64::from(byte)).wrapping_mul(PRIME);
        second = (second.rotate_left(7) ^ u64::from(byte)).wrapping_mul(PRIME);
    }
    format!("{first:016x}{second:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_always_gets_the_same_name_and_other_text_another() {
        assert_eq!(
            stable_key("https://s.test/a.vtt"),
            stable_key("https://s.test/a.vtt")
        );
        assert_ne!(
            stable_key("https://s.test/a.vtt"),
            stable_key("https://s.test/b.vtt")
        );
        assert_eq!(stable_key("").len(), 32);
    }

    #[test]
    fn a_name_never_changes_between_versions() {
        // Fixed on purpose: cached files are found by these names.
        assert_eq!(stable_key("a"), "af63dc4c8601ec8c4f4ebfc6a04bf179");
    }
}
