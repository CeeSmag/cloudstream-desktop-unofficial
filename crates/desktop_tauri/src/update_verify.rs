//! Proof that a downloaded update is the file the release was signed for. The release carries
//! `SHA256SUMS` (one `<sha256>  <file name>` line per file) and `SHA256SUMS.minisig`, a minisign
//! signature of that list made with the owner's secret key. The app checks the signature with the
//! public key built into it, then the downloaded file's SHA-256 against the list.

use std::fmt::Write;

use minisign_verify::{PublicKey, Signature};

use crate::update_error::UpdateError;

/// The release-signing public key built into the app (a minisign `.pub` file). Until the owner
/// puts the real key there (`docs/RELEASING.md`) it holds no key, and updates are refused.
pub(crate) const UPDATE_SIGNING_KEY: &str = include_str!("../update_signing_key.pub");

/// Length of a SHA-256 written as hex.
const SHA256_HEX_LENGTH: usize = 64;

/// Legacy (not pre-hashed) minisign signatures are refused: every minisign since 0.8 pre-hashes.
const ALLOW_LEGACY_SIGNATURES: bool = false;

/// The checksum list of a release whose signature was checked.
#[derive(Debug)]
pub(crate) struct ReleaseChecksums {
    /// `(file name, lowercase hex SHA-256)` in the list's order.
    entries: Vec<(String, String)>,
}

impl ReleaseChecksums {
    /// The expected SHA-256 (lowercase hex) of the release file `file`.
    pub(crate) fn expected_for(&self, file: &str) -> Result<&str, UpdateError> {
        self.entries
            .iter()
            .find(|(name, _)| name == file)
            .map(|(_, sha256)| sha256.as_str())
            .ok_or_else(|| UpdateError::MissingChecksum {
                file: file.to_string(),
            })
    }
}

/// Checks `signature` (the `.minisig` text) over `sums` (the `SHA256SUMS` text) with `public_key`
/// (a minisign public key file's text) and reads the list.
pub(crate) fn verify_checksums(
    sums: &str,
    signature: &str,
    public_key: &str,
) -> Result<ReleaseChecksums, UpdateError> {
    let public_key = PublicKey::decode(public_key).map_err(UpdateError::NoUpdateKey)?;
    let signature = Signature::decode(signature).map_err(UpdateError::Signature)?;
    public_key
        .verify(sums.as_bytes(), &signature, ALLOW_LEGACY_SIGNATURES)
        .map_err(UpdateError::Signature)?;
    parse_checksums(sums)
}

/// Reads `sha256sum` output: `<hex>  <name>`, or `<hex> *<name>` in binary mode.
fn parse_checksums(sums: &str) -> Result<ReleaseChecksums, UpdateError> {
    let entries = sums
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(parse_checksum_line)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReleaseChecksums { entries })
}

fn parse_checksum_line(line: &str) -> Result<(String, String), UpdateError> {
    let unreadable = || UpdateError::ChecksumList(format!("line '{line}'"));
    let (sha256, name) = line
        .split_once(char::is_whitespace)
        .ok_or_else(unreadable)?;
    let name = name.trim_start().trim_start_matches('*');
    let is_sha256 =
        sha256.len() == SHA256_HEX_LENGTH && sha256.chars().all(|digit| digit.is_ascii_hexdigit());
    if !is_sha256 || name.is_empty() {
        return Err(unreadable());
    }
    Ok((name.to_string(), sha256.to_ascii_lowercase()))
}

/// `bytes` as lowercase hex.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
            // Writing to a String cannot fail.
            let _ = write!(text, "{byte:02x}");
            text
        })
}

/// Fails unless `actual` (the downloaded file's SHA-256) is `expected` (hex, any case).
pub(crate) fn check_download(expected: &str, actual: &[u8], file: &str) -> Result<(), UpdateError> {
    if expected.eq_ignore_ascii_case(&to_hex(actual)) {
        Ok(())
    } else {
        Err(UpdateError::ChecksumMismatch {
            file: file.to_string(),
        })
    }
}

#[cfg(test)]
#[path = "update_verify_tests.rs"]
mod tests;
