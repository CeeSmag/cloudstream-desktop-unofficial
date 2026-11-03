//! Finds which subtitle languages a stream offers by reading the start of it: the track list of a
//! Matroska file, the subtitle renditions of an HLS master playlist, the text adaptation sets of
//! a DASH manifest. Streams that say nothing at the start (most MP4 files) yield nothing.

const EBML_MAGIC: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];
const SEGMENT: u32 = 0x1853_8067;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_TYPE: u32 = 0x83;
const LANGUAGE: u32 = 0x22_B59C;
const LANGUAGE_BCP47: u32 = 0x22_B59D;
const TRACK_NAME: u32 = 0x53_6E;
const SUBTITLE_TRACK: u64 = 17;
/// What a Matroska track without a language element is in.
const DEFAULT_MATROSKA_LANGUAGE: &str = "eng";

/// The languages of the subtitles inside a stream, as the stream names them (`eng`, `en`,
/// `English`), without repeats, judged from its first bytes.
pub fn subtitle_languages(head: &[u8]) -> Vec<String> {
    let found = if head.starts_with(&EBML_MAGIC) {
        matroska_languages(head)
    } else {
        let text = String::from_utf8_lossy(head);
        let text = text.trim_start();
        if text.starts_with("#EXTM3U") {
            hls_languages(text)
        } else if text.starts_with("<?xml") || text.starts_with("<MPD") {
            dash_languages(text)
        } else {
            Vec::new()
        }
    };
    let mut unique: Vec<String> = Vec::new();
    for language in found {
        if !unique
            .iter()
            .any(|seen| seen.eq_ignore_ascii_case(&language))
        {
            unique.push(language);
        }
    }
    unique
}

/// Reads an EBML variable-length number. With `keep_marker` the length bits stay in the value
/// (element ids); without, they are removed (sizes). Returns the value and how many bytes it took.
fn read_vint(bytes: &[u8], keep_marker: bool) -> Option<(u64, usize)> {
    let first = *bytes.first()?;
    let length = first.leading_zeros() as usize + 1;
    if length > 8 {
        return None;
    }
    let marker_free = if length == 8 {
        0
    } else {
        first & (0xFF >> length)
    };
    let mut value = u64::from(if keep_marker { first } else { marker_free });
    for byte in bytes.get(1..length)? {
        value = (value << 8) | u64::from(*byte);
    }
    Some((value, length))
}

/// The next element: its id, its body (cut short where the data ends) and what follows it.
fn next_element(bytes: &[u8]) -> Option<(u32, &[u8], &[u8])> {
    let (id, id_length) = read_vint(bytes, true)?;
    let after_id = bytes.get(id_length..)?;
    let (size, size_length) = read_vint(after_id, false)?;
    let content = after_id.get(size_length..)?;
    let unknown_size = size == (1u64 << (7 * size_length)) - 1;
    let id = u32::try_from(id).ok()?;
    let wanted = if unknown_size {
        content.len()
    } else {
        usize::try_from(size).unwrap_or(usize::MAX)
    };
    // Only the containers that lead to the tracks may be cut short by the end of the data; any
    // other element that is cut short is incomplete and not trusted.
    if wanted > content.len() && !matches!(id, SEGMENT | TRACKS) {
        return None;
    }
    let (body, rest) = content.split_at_checked(wanted.min(content.len()))?;
    Some((id, body, rest))
}

fn elements(mut bytes: &[u8]) -> impl Iterator<Item = (u32, &[u8])> + '_ {
    std::iter::from_fn(move || {
        let (id, body, rest) = next_element(bytes)?;
        bytes = rest;
        Some((id, body))
    })
}

fn children_of<'a>(
    parents: impl Iterator<Item = (u32, &'a [u8])>,
    wanted: u32,
) -> impl Iterator<Item = &'a [u8]> {
    parents
        .filter(move |(id, _)| *id == wanted)
        .map(|(_, body)| body)
}

fn matroska_languages(head: &[u8]) -> Vec<String> {
    let segments = children_of(elements(head), SEGMENT);
    let tracks = segments.flat_map(|segment| children_of(elements(segment), TRACKS));
    let entries = tracks.flat_map(|list| children_of(elements(list), TRACK_ENTRY));
    entries.filter_map(subtitle_language_of_track).collect()
}

/// The language of one Matroska track, if it is a subtitle track.
fn subtitle_language_of_track(entry: &[u8]) -> Option<String> {
    let (mut kind, mut language, mut bcp47, mut name) = (None, None, None, None);
    for (id, body) in elements(entry) {
        match id {
            TRACK_TYPE => kind = Some(body.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b))),
            LANGUAGE => language = Some(text_of(body)),
            LANGUAGE_BCP47 => bcp47 = Some(text_of(body)),
            TRACK_NAME => name = Some(text_of(body)),
            _ => {}
        }
    }
    if kind != Some(SUBTITLE_TRACK) {
        return None;
    }
    let code = bcp47
        .or(language)
        .unwrap_or_else(|| DEFAULT_MATROSKA_LANGUAGE.to_owned());
    if matches!(code.to_ascii_lowercase().as_str(), "und" | "") {
        return name.filter(|name| !name.is_empty());
    }
    Some(code)
}

fn text_of(body: &[u8]) -> String {
    String::from_utf8_lossy(body)
        .trim_end_matches('\0')
        .trim()
        .to_owned()
}

/// The value of `name="..."` inside `text`.
fn attribute(text: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let start = text.find(&key)? + key.len();
    let rest = text.get(start..)?;
    Some(rest.get(..rest.find('"')?)?.to_owned())
}

fn hls_languages(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with("#EXT-X-MEDIA:") && line.contains("TYPE=SUBTITLES"))
        .filter_map(|line| attribute(line, "LANGUAGE").or_else(|| attribute(line, "NAME")))
        .collect()
}

fn dash_languages(text: &str) -> Vec<String> {
    text.split("<AdaptationSet")
        .skip(1)
        .filter_map(|chunk| chunk.split('>').next())
        .filter(|tag| {
            tag.contains("contentType=\"text\"")
                || tag.contains("mimeType=\"text/")
                || tag.contains("mimeType=\"application/ttml")
        })
        .filter_map(|tag| attribute(tag, "lang"))
        .collect()
}

#[cfg(test)]
#[path = "subtitle_scan_tests.rs"]
mod tests;
