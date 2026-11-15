//! The secret in every proxied address, so that only the player that was handed one can use it.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

/// How many 64-bit words a token is made of (two give 128 bits).
const TOKEN_WORDS: u64 = 2;

/// A new token: 32 hexadecimal digits that cannot be guessed from the earlier ones.
///
/// There is no random-number crate in the project. The standard library seeds `RandomState` from
/// the operating system's random source; hashing a counter under that secret key gives words
/// nobody can predict without it. The token guards a loopback port against other programs on the
/// PC, not against someone who can already read this process's memory.
pub(crate) fn new_token(counter: u64) -> String {
    (0..TOKEN_WORDS)
        .map(|word| {
            let mut hasher = RandomState::new().build_hasher();
            hasher.write_u64(counter);
            hasher.write_u64(word);
            format!("{:016x}", hasher.finish())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn new_token_is_32_hex_digits() {
        let token = new_token(1);
        assert_eq!(token.len(), 32);
        assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn new_token_never_repeats_even_for_the_same_counter() {
        let tokens: HashSet<String> = (0..1000).map(|_| new_token(7)).collect();
        assert_eq!(tokens.len(), 1000);
    }
}
