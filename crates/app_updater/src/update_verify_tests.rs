//! T1 tests of the release signature and checksum checks. Each test signs with a key pair made for
//! that test only; the release key never appears in tests.

use minisign::KeyPair;

use super::*;

const INSTALLER: &str = "CloudStream_0.5.0_x64-setup.exe";
const INSTALLER_SHA: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const PORTABLE: &str = "CloudStream_0.5.0_x64_portable.exe";
const PORTABLE_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// SHA-256 of the three bytes "abc", the value `INSTALLER_SHA` stands for.
const ABC_DIGEST: [u8; 32] = [
    0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
    0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
];

/// What `sha256sum` writes for the two files (binary-mode `*` marker on the second line).
fn release_sums() -> String {
    format!("{INSTALLER_SHA}  {INSTALLER}\n{PORTABLE_SHA} *{PORTABLE}\n")
}

/// A freshly made test key pair and its public key file's text.
fn test_key() -> (KeyPair, String) {
    let keys = KeyPair::generate_unencrypted_keypair().unwrap();
    let public_key = keys.pk.to_box().unwrap().to_string();
    (keys, public_key)
}

/// The `.minisig` text for `sums`, signed with `keys`.
fn sign(keys: &KeyPair, sums: &str) -> String {
    minisign::sign(Some(&keys.pk), &keys.sk, sums.as_bytes(), None, None)
        .unwrap()
        .to_string()
}

#[test]
fn verify_checksums_signed_list_returns_each_files_checksum() {
    let (keys, public_key) = test_key();
    let sums = release_sums();
    let checksums = verify_checksums(&sums, &sign(&keys, &sums), &public_key).unwrap();
    assert_eq!(checksums.expected_for(INSTALLER).unwrap(), INSTALLER_SHA);
    assert_eq!(checksums.expected_for(PORTABLE).unwrap(), PORTABLE_SHA);
}

#[test]
fn verify_checksums_windows_line_endings_and_blank_lines_are_read() {
    let (keys, public_key) = test_key();
    let sums = format!("\r\n{INSTALLER_SHA}  {INSTALLER}\r\n\r\n");
    let checksums = verify_checksums(&sums, &sign(&keys, &sums), &public_key).unwrap();
    assert_eq!(checksums.expected_for(INSTALLER).unwrap(), INSTALLER_SHA);
}

#[test]
fn verify_checksums_list_changed_after_signing_returns_signature_error() {
    let (keys, public_key) = test_key();
    let sums = release_sums();
    let signature = sign(&keys, &sums);
    let changed = sums.replacen("ba78", "0a78", 1);
    let result = verify_checksums(&changed, &signature, &public_key);
    assert!(matches!(result, Err(UpdateError::Signature(_))));
}

#[test]
fn verify_checksums_signed_with_another_key_returns_signature_error() {
    let (keys, _) = test_key();
    let (_, other_public_key) = test_key();
    let sums = release_sums();
    let result = verify_checksums(&sums, &sign(&keys, &sums), &other_public_key);
    assert!(matches!(result, Err(UpdateError::Signature(_))));
}

#[test]
fn verify_checksums_garbled_signature_returns_signature_error() {
    let (_, public_key) = test_key();
    let result = verify_checksums(&release_sums(), "not a signature", &public_key);
    assert!(matches!(result, Err(UpdateError::Signature(_))));
}

#[test]
fn verify_checksums_key_file_without_key_returns_no_update_key() {
    let (keys, _) = test_key();
    let sums = release_sums();
    let placeholder = "untrusted comment: no release key yet\n";
    let result = verify_checksums(&sums, &sign(&keys, &sums), placeholder);
    assert!(matches!(result, Err(UpdateError::NoUpdateKey(_))));
}

#[test]
fn verify_checksums_signed_line_without_name_returns_checksum_list_error() {
    let (keys, public_key) = test_key();
    let sums = format!("{INSTALLER_SHA}\n");
    let result = verify_checksums(&sums, &sign(&keys, &sums), &public_key);
    assert!(matches!(result, Err(UpdateError::ChecksumList(_))));
}

#[test]
fn verify_checksums_signed_line_with_short_hash_returns_checksum_list_error() {
    let (keys, public_key) = test_key();
    let sums = format!("abc123  {INSTALLER}\n");
    let result = verify_checksums(&sums, &sign(&keys, &sums), &public_key);
    assert!(matches!(result, Err(UpdateError::ChecksumList(_))));
}

#[test]
fn expected_for_file_not_in_list_returns_missing_checksum() {
    let (keys, public_key) = test_key();
    let sums = release_sums();
    let checksums = verify_checksums(&sums, &sign(&keys, &sums), &public_key).unwrap();
    let result = checksums.expected_for("CloudStream-0.5.0-x86_64.AppImage");
    assert!(matches!(
        result,
        Err(UpdateError::MissingChecksum { ref file }) if file == "CloudStream-0.5.0-x86_64.AppImage"
    ));
}

#[test]
fn check_download_same_digest_passes() {
    assert!(check_download(INSTALLER_SHA, &ABC_DIGEST, INSTALLER).is_ok());
    assert!(check_download(&INSTALLER_SHA.to_uppercase(), &ABC_DIGEST, INSTALLER).is_ok());
}

#[test]
fn check_download_other_digest_returns_checksum_mismatch() {
    let result = check_download(PORTABLE_SHA, &ABC_DIGEST, INSTALLER);
    assert!(matches!(
        result,
        Err(UpdateError::ChecksumMismatch { ref file }) if file == INSTALLER
    ));
}

#[test]
fn update_signing_key_built_in_file_is_a_minisign_public_key() {
    let key = minisign_verify::PublicKey::decode(UPDATE_SIGNING_KEY);
    assert!(
        key.is_ok(),
        "update_signing_key.pub must hold the owner's minisign public key"
    );
}

/// End-to-end check of `scripts/sign_release.sh` against the app's own verifier:
/// `CS_TEST_SIGNED_RELEASE=<signed folder> [CS_TEST_RELEASE_PUBLIC_KEY=<.pub>]
/// cargo test -p app_updater -- --ignored sign_release_script`. Without the key variable the key
/// built into the app is used, as for a real release.
#[test]
#[ignore = "needs a folder signed by scripts/sign_release.sh"]
fn verify_checksums_sign_release_script_output_is_accepted() {
    use sha2::{Digest, Sha256};

    let folder = std::path::PathBuf::from(std::env::var("CS_TEST_SIGNED_RELEASE").unwrap());
    let public_key = match std::env::var("CS_TEST_RELEASE_PUBLIC_KEY") {
        Ok(path) => std::fs::read_to_string(path).unwrap(),
        Err(_) => UPDATE_SIGNING_KEY.to_string(),
    };
    let sums = std::fs::read_to_string(folder.join("SHA256SUMS")).unwrap();
    let signature = std::fs::read_to_string(folder.join("SHA256SUMS.minisig")).unwrap();

    let checksums = verify_checksums(&sums, &signature, &public_key).unwrap();
    assert!(!checksums.entries.is_empty());
    for (name, expected) in &checksums.entries {
        let digest = Sha256::digest(std::fs::read(folder.join(name)).unwrap());
        check_download(expected, &digest, name).unwrap();
    }
}
