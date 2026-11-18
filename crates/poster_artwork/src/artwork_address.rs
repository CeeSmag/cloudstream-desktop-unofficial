//! Turns the poster address a provider gives into one worth downloading.
//!
//! Providers built on TMDB (CineStream and friends) hand out `/t/p/original/` images: the full
//! upload, often several megabytes for a tile that is shown 180 pixels wide. TMDB serves every
//! image at fixed widths too, so the address is rewritten to the poster width the screen needs.

use crate::ArtworkError;

/// The TMDB width asked for instead of `original` (or anything wider).
const TMDB_POSTER_SIZE: &str = "w342";
/// The TMDB image host.
const TMDB_IMAGE_HOST: &str = "image.tmdb.org";
/// TMDB widths up to this are small enough to keep.
const TMDB_MAX_KEPT_WIDTH: u32 = 500;

/// The address to download for `raw`, or why it cannot be downloaded.
pub fn poster_address(raw: &str) -> Result<String, ArtworkError> {
    let trimmed = raw.trim();
    let absolute = match trimmed.strip_prefix("//") {
        Some(rest) => format!("https://{rest}"),
        None => trimmed.to_owned(),
    };
    if !(absolute.starts_with("https://") || absolute.starts_with("http://")) {
        return Err(ArtworkError::Unsupported(scheme_of(&absolute)));
    }
    Ok(percent_encode_unsafe(&smaller_tmdb_size(&absolute)))
}

fn scheme_of(address: &str) -> String {
    match address.split_once(':') {
        Some((scheme, _)) if !scheme.is_empty() && scheme.len() < 16 => {
            format!("{scheme}: address")
        }
        _ => "address without http(s)".to_owned(),
    }
}

/// `…image.tmdb.org/t/p/original/x.jpg` → `…/t/p/w342/x.jpg`; other addresses unchanged.
fn smaller_tmdb_size(address: &str) -> String {
    let Some((scheme, rest)) = address.split_once("://") else {
        return address.to_owned();
    };
    let Some(path) = rest.strip_prefix(TMDB_IMAGE_HOST) else {
        return address.to_owned();
    };
    let Some((size, file)) = path
        .strip_prefix("/t/p/")
        .and_then(|tail| tail.split_once('/'))
    else {
        return address.to_owned();
    };
    if !is_too_wide(size) {
        return address.to_owned();
    }
    format!("{scheme}://{TMDB_IMAGE_HOST}/t/p/{TMDB_POSTER_SIZE}/{file}")
}

fn is_too_wide(size: &str) -> bool {
    if size == "original" {
        return true;
    }
    size.strip_prefix('w')
        .and_then(|width| width.parse::<u32>().ok())
        .is_some_and(|width| width > TMDB_MAX_KEPT_WIDTH)
}

/// Encodes spaces, quotes and non-ASCII characters, which providers leave raw and the HTTP
/// client refuses; `%` escapes already present are kept.
fn percent_encode_unsafe(address: &str) -> String {
    let mut encoded = String::with_capacity(address.len());
    for character in address.chars() {
        if character.is_ascii_graphic() && !"\"<>\\^`{|}".contains(character) {
            encoded.push(character);
        } else {
            let mut buffer = [0u8; 4];
            for byte in character.encode_utf8(&mut buffer).bytes() {
                encoded.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poster_address_tmdb_original_becomes_poster_width() {
        let address = poster_address("https://image.tmdb.org/t/p/original//zG7h.jpg");
        assert_eq!(
            address,
            Ok("https://image.tmdb.org/t/p/w342//zG7h.jpg".to_owned())
        );
    }

    #[test]
    fn poster_address_tmdb_wide_size_shrinks_small_size_stays() {
        assert_eq!(
            poster_address("https://image.tmdb.org/t/p/w1280/a.jpg"),
            Ok("https://image.tmdb.org/t/p/w342/a.jpg".to_owned())
        );
        assert_eq!(
            poster_address("https://image.tmdb.org/t/p/w185/a.jpg"),
            Ok("https://image.tmdb.org/t/p/w185/a.jpg".to_owned())
        );
    }

    #[test]
    fn poster_address_other_host_with_tmdb_like_path_is_unchanged() {
        let raw = "https://img.example/t/p/original/a.jpg";
        assert_eq!(poster_address(raw), Ok(raw.to_owned()));
    }

    #[test]
    fn poster_address_protocol_relative_gets_https() {
        assert_eq!(
            poster_address("//cdn.example/p.png"),
            Ok("https://cdn.example/p.png".to_owned())
        );
    }

    #[test]
    fn poster_address_spaces_and_non_ascii_are_encoded_escapes_kept() {
        assert_eq!(
            poster_address(" https://cdn.example/My Show é%20x.jpg "),
            Ok("https://cdn.example/My%20Show%20%C3%A9%20x.jpg".to_owned())
        );
    }

    #[test]
    fn poster_address_non_web_scheme_is_unsupported() {
        assert!(matches!(
            poster_address("data:image/png;base64,AAAA"),
            Err(ArtworkError::Unsupported(reason)) if reason == "data: address"
        ));
        assert!(matches!(
            poster_address(""),
            Err(ArtworkError::Unsupported(_))
        ));
    }
}
