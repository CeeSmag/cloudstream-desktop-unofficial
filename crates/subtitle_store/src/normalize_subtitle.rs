//! Turns whatever a host sent as a subtitle into clean text with the right file type: unpacks a
//! zip or gzip answer, reads the text in its real encoding (UTF-8, UTF-16 with a byte order mark,
//! or the old Windows-1252) and says whether it is VTT, ASS, SMI, a playlist or plain SRT.

use std::io::Read;

use flate2::bufread::DeflateDecoder;
use flate2::read::GzDecoder;

/// Subtitle text ready to be written to a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedSubtitle {
    /// The text, decoded.
    pub text: String,
    /// File extension without the dot: `srt`, `vtt`, `ass`, `smi` or `m3u8`.
    pub extension: &'static str,
}

/// Why an answer is not a usable subtitle.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NormalizeError {
    /// Nothing, or only blanks.
    #[error("the answer was empty")]
    Empty,
    /// A web page (an error page or a login page) where a subtitle was expected.
    #[error("the answer is a web page, not a subtitle")]
    WebPage,
    /// A zip file without a subtitle in it.
    #[error("the archive holds no subtitle file")]
    NoSubtitleInArchive,
    /// An archive that could not be unpacked.
    #[error("the archive could not be unpacked: {0}")]
    Archive(String),
}

const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];
const ZIP_LOCAL_HEADER: [u8; 4] = [0x50, 0x4B, 0x03, 0x04];
const ZIP_HEADER_LENGTH: usize = 30;
const ZIP_STORED: u16 = 0;
const ZIP_DEFLATED: u16 = 8;
const ZIP_HAS_DATA_DESCRIPTOR: u16 = 0x0008;
const ZIP_DESCRIPTOR_LENGTH: usize = 12;
const ZIP_DESCRIPTOR_SIGNATURE: [u8; 4] = [0x50, 0x4B, 0x07, 0x08];
/// How much of the start of the text decides what kind of file it is.
const SNIFF_CHARACTERS: usize = 500;
/// The most a subtitle may grow when unpacked; a real one is far smaller.
const MAX_UNPACKED_BYTES: u64 = 16 * 1024 * 1024;
const SUBTITLE_EXTENSIONS: [&str; 5] = [".srt", ".vtt", ".ass", ".ssa", ".sub"];

/// The Windows-1252 characters for the bytes 0x80 to 0x9F (the rest equals Latin-1).
const WINDOWS_1252: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
];

/// Cleans up a downloaded subtitle.
pub fn normalize_subtitle(bytes: &[u8]) -> Result<NormalizedSubtitle, NormalizeError> {
    let unpacked = unpack(bytes)?;
    let text = decode_text(&unpacked);
    if text.trim().is_empty() {
        return Err(NormalizeError::Empty);
    }
    if looks_like_web_page(&text) {
        return Err(NormalizeError::WebPage);
    }
    Ok(NormalizedSubtitle {
        extension: detect_extension(&text),
        text,
    })
}

fn unpack(bytes: &[u8]) -> Result<Vec<u8>, NormalizeError> {
    if bytes.starts_with(&GZIP_MAGIC) {
        let mut unpacked = Vec::new();
        GzDecoder::new(bytes)
            .take(MAX_UNPACKED_BYTES)
            .read_to_end(&mut unpacked)
            .map_err(|error| NormalizeError::Archive(error.to_string()))?;
        return Ok(unpacked);
    }
    if bytes.starts_with(&ZIP_LOCAL_HEADER) {
        return first_subtitle_in_zip(bytes);
    }
    Ok(bytes.to_vec())
}

fn first_subtitle_in_zip(bytes: &[u8]) -> Result<Vec<u8>, NormalizeError> {
    let mut position = 0;
    while let Some(entry) = read_zip_entry(bytes, position)? {
        if is_subtitle_name(&entry.name) && !entry.content.is_empty() {
            return Ok(entry.content);
        }
        position = entry.next;
    }
    Err(NormalizeError::NoSubtitleInArchive)
}

fn is_subtitle_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower);
    !lower.contains("__macosx")
        && !file.starts_with('.')
        && SUBTITLE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

struct ZipEntry {
    name: String,
    content: Vec<u8>,
    /// Where the next entry's header starts.
    next: usize,
}

fn little_endian(bytes: &[u8], at: usize, length: usize) -> Option<usize> {
    let slice = bytes.get(at..at.checked_add(length)?)?;
    Some(
        slice
            .iter()
            .rev()
            .fold(0usize, |value, byte| (value << 8) | usize::from(*byte)),
    )
}

/// Reads the zip entry whose local header starts at `at`; `None` where the entries end.
fn read_zip_entry(bytes: &[u8], at: usize) -> Result<Option<ZipEntry>, NormalizeError> {
    let broken = || NormalizeError::Archive("a header is cut short".to_owned());
    if bytes.get(at..at + 4) != Some(&ZIP_LOCAL_HEADER[..]) {
        return Ok(None);
    }
    let field =
        |offset: usize, length: usize| little_endian(bytes, at + offset, length).ok_or_else(broken);
    let (flags, method) = (field(6, 2)?, field(8, 2)?);
    let (compressed, name_length, extra_length) = (field(18, 4)?, field(26, 2)?, field(28, 2)?);
    let name_start = at + ZIP_HEADER_LENGTH;
    let data_start = name_start + name_length + extra_length;
    let name_bytes = bytes
        .get(name_start..name_start + name_length)
        .ok_or_else(broken)?;
    let name = String::from_utf8_lossy(name_bytes).into_owned();
    let data = bytes.get(data_start..).ok_or_else(broken)?;
    let (content, used) = match u16::try_from(method) {
        Ok(ZIP_STORED) => {
            let stored = data.get(..compressed).ok_or_else(broken)?;
            (stored.to_vec(), compressed)
        }
        Ok(ZIP_DEFLATED) => inflate(data)?,
        _ => {
            return Err(NormalizeError::Archive(format!(
                "unsupported compression {method}"
            )))
        }
    };
    let mut next = data_start + used;
    if flags & usize::from(ZIP_HAS_DATA_DESCRIPTOR) != 0 {
        let signed = bytes.get(next..next + 4) == Some(&ZIP_DESCRIPTOR_SIGNATURE[..]);
        next += ZIP_DESCRIPTOR_LENGTH + if signed { 4 } else { 0 };
    }
    Ok(Some(ZipEntry {
        name,
        content,
        next,
    }))
}

/// Inflates a raw deflate stream; also says how many bytes of `data` it took.
fn inflate(data: &[u8]) -> Result<(Vec<u8>, usize), NormalizeError> {
    let mut rest = data;
    let mut content = Vec::new();
    DeflateDecoder::new(&mut rest)
        .take(MAX_UNPACKED_BYTES)
        .read_to_end(&mut content)
        .map_err(|error| NormalizeError::Archive(error.to_string()))?;
    Ok((content, data.len() - rest.len()))
}

fn decode_text(bytes: &[u8]) -> String {
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, u16::from_be_bytes),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => bytes.iter().map(|byte| windows_1252(*byte)).collect(),
        },
    }
}

fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| unit(*pair))
        .collect();
    String::from_utf16_lossy(&units)
}

fn windows_1252(byte: u8) -> char {
    match byte {
        0x80..=0x9F => WINDOWS_1252
            .get(usize::from(byte - 0x80))
            .copied()
            .unwrap_or('\u{FFFD}'),
        other => char::from(other),
    }
}

fn looks_like_web_page(text: &str) -> bool {
    let start = text.trim_start().to_ascii_lowercase();
    start.starts_with("<!doctype html") || start.starts_with("<html")
}

fn detect_extension(text: &str) -> &'static str {
    let sample: String = text.trim_start().chars().take(SNIFF_CHARACTERS).collect();
    let lower = sample.to_ascii_lowercase();
    if lower.starts_with("webvtt") {
        "vtt"
    } else if lower.contains("[script info]") || lower.contains("[v4+ styles]") {
        "ass"
    } else if lower.contains("<sami>") {
        "smi"
    } else if lower.starts_with("#extm3u") || lower.contains("#ext-x-") {
        "m3u8"
    } else {
        "srt"
    }
}

#[cfg(test)]
#[path = "normalize_subtitle_tests.rs"]
mod tests;
