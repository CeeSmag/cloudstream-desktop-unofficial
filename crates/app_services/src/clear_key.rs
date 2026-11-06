//! Normalization for ClearKey CENC decryption keys and pipe-annotated stream URLs.

use core_model::PlayableLink;

/// Converts a ClearKey `key` (in 32-char hex, Base64, Base64URL, or `kid:key` form) into the
/// 32-character lowercase hex string expected by `libmpv` / FFmpeg (`decryption_key`).
pub fn normalize_clear_key(_kid: Option<&str>, key: Option<&str>) -> Option<String> {
    let raw = key?.trim();
    if raw.is_empty() {
        return None;
    }
    let key_part = match raw.split_once(':') {
        Some((_, right)) => right.trim(),
        None => raw,
    };
    decode_key_to_hex(key_part)
}

/// Converts a single 16-byte key token (hex, Base64, or Base64URL) to 32 lowercase hex chars.
pub fn decode_key_to_hex(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped: String = trimmed.chars().filter(|c| *c != '-').collect();
    if stripped.len() == 32 && stripped.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(stripped.to_ascii_lowercase());
    }
    let bytes = decode_base64_16(trimmed)?;
    Some(bytes_to_hex(&bytes))
}

fn decode_base64_16(input: &str) -> Option<[u8; 16]> {
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .map(base64_sextet)
        .collect::<Option<Vec<u8>>>()?;
    if clean.len() != 22 && clean.len() != 24 {
        return None;
    }
    let mut out = [0u8; 16];
    let mut out_idx = 0usize;
    let mut acc = 0u32;
    let mut bits = 0u32;
    for sextet in clean {
        acc = (acc << 6) | u32::from(sextet);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            if let Some(slot) = out.get_mut(out_idx) {
                *slot = ((acc >> bits) & 0xFF) as u8;
                out_idx += 1;
            }
        }
    }
    (out_idx == 16).then_some(out)
}

fn base64_sextet(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let hi = HEX.get(usize::from(b >> 4)).copied().unwrap_or(b'0');
        let lo = HEX.get(usize::from(b & 0x0F)).copied().unwrap_or(b'0');
        out.push(char::from(hi));
        out.push(char::from(lo));
    }
    out
}

/// Splits pipe-appended stream options (`url|User-Agent=...&Referer=...&drmLicense=kid:key`)
/// out of `link.url` into `link.headers`, `link.referer`, `link.kid`, and `link.key`.
pub fn normalize_pipe_link(mut link: PlayableLink) -> PlayableLink {
    let Some((clean_url, options)) = link.url.split_once('|') else {
        return link;
    };
    let clean_url = clean_url.trim().to_owned();
    let options = options.trim().to_owned();
    link.url = clean_url;
    if options.is_empty() {
        return link;
    }
    if !options.contains('=') {
        insert_header_if_missing(&mut link, "User-Agent", &options);
        return link;
    }
    for pair in options.split('&') {
        apply_pipe_pair(&mut link, pair.trim());
    }
    link
}

fn apply_pipe_pair(link: &mut PlayableLink, pair: &str) {
    let Some((raw_key, raw_val)) = pair.split_once('=') else {
        return;
    };
    let key = raw_key.trim();
    let val = raw_val.trim();
    if key.is_empty() || val.is_empty() {
        return;
    }
    match key.to_ascii_lowercase().as_str() {
        "referer" | "referrer" => {
            if link.referer.is_empty() {
                link.referer = val.to_owned();
            }
        }
        "user-agent" | "useragent" => insert_header_if_missing(link, "User-Agent", val),
        "origin" => insert_header_if_missing(link, "Origin", val),
        "cookie" => insert_header_if_missing(link, "Cookie", val),
        "authorization" => insert_header_if_missing(link, "Authorization", val),
        "drmscheme" => {}
        "drmlicense" => apply_drm_license(link, val),
        _ => insert_header_if_missing(link, key, val),
    }
}

fn apply_drm_license(link: &mut PlayableLink, license: &str) {
    if link.key.is_some() {
        return;
    }
    if let Some((kid, key)) = license.split_once(':') {
        if link.kid.is_none() && !kid.trim().is_empty() {
            link.kid = Some(kid.trim().to_owned());
        }
        if !key.trim().is_empty() {
            link.key = Some(key.trim().to_owned());
        }
    } else if !license.starts_with("http") {
        link.key = Some(license.to_owned());
    }
}

fn insert_header_if_missing(link: &mut PlayableLink, name: &str, value: &str) {
    let exists = link
        .headers
        .keys()
        .any(|existing| existing.eq_ignore_ascii_case(name));
    if !exists {
        link.headers.insert(name.to_owned(), value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64url_clear_key_from_livxow_to_hex() {
        // Real KID/Key pair from LivXow Live Events (TNT SPORTS FHD cenc.mpd).
        let kid = decode_key_to_hex("8994QwgK50O_hl3F_fZMaA");
        let key = normalize_clear_key(
            Some("8994QwgK50O_hl3F_fZMaA"),
            Some("VnyGO8Eut0eI6nSIjAQuGw"),
        );
        assert_eq!(kid.as_deref(), Some("f3df7843080ae743bf865dc5fdf64c68"));
        assert_eq!(key.as_deref(), Some("567c863bc12eb74788ea74888c042e1b"));
    }

    #[test]
    fn preserves_hex_and_kid_colon_key_formats() {
        assert_eq!(
            normalize_clear_key(None, Some("F3DF7843-080A-E743-BF86-5DC5FDF64C68")).as_deref(),
            Some("f3df7843080ae743bf865dc5fdf64c68")
        );
        assert_eq!(
            normalize_clear_key(
                None,
                Some("f3df7843080ae743bf865dc5fdf64c68:567c863bc12eb74788ea74888c042e1b")
            )
            .as_deref(),
            Some("567c863bc12eb74788ea74888c042e1b")
        );
        assert_eq!(normalize_clear_key(None, Some("not-a-valid-key")), None);
        assert_eq!(normalize_clear_key(None, None), None);
    }

    #[test]
    fn extracts_pipe_headers_and_drm_license_from_live_stream_url() {
        let raw = PlayableLink {
            url: "https://otte.cache.aiv-cdn.net/live/cenc.mpd|user-agent=Mozilla/5.0&referer=https://livxow.com/&drmScheme=clearkey&drmLicense=f3df7843080ae743bf865dc5fdf64c68:567c863bc12eb74788ea74888c042e1b".into(),
            link_type: "DASH".into(),
            ..PlayableLink::default()
        };
        let clean = normalize_pipe_link(raw);
        assert_eq!(clean.url, "https://otte.cache.aiv-cdn.net/live/cenc.mpd");
        assert_eq!(clean.referer, "https://livxow.com/");
        assert_eq!(
            clean.headers.get("User-Agent").map(String::as_str),
            Some("Mozilla/5.0")
        );
        assert_eq!(
            clean.kid.as_deref(),
            Some("f3df7843080ae743bf865dc5fdf64c68")
        );
        assert_eq!(
            clean.key.as_deref(),
            Some("567c863bc12eb74788ea74888c042e1b")
        );
    }
}
