use std::path::{Component, Path, PathBuf};

use core_model::stable_key;

use crate::RepoError;

const MAX_DIRECT_LEN: usize = 120;
const MAX_SLUG_LEN: usize = 64;
const HASH_PREFIX_LEN: usize = 12;

const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Turns a plugin's manifest `internal_name` into a safe `<stem>.cs3` filename.
///
/// Path separators, traversal sequences, control characters, and blank names are always refused.
/// Clean `[A-Za-z0-9_-]` names keep `<internal_name>.cs3` for backwards compatibility; other
/// legitimate names (such as `18+` or `CNC Verse`) map deterministically to `<slug>_<hash>.cs3`
/// so the on-disk filename boundary remains strictly `^[A-Za-z0-9_-]+\.cs3$`.
pub fn plugin_file_name(internal_name: &str) -> Result<String, RepoError> {
    reject_hostile_name(internal_name)?;
    if is_direct_safe_stem(internal_name) {
        return Ok(format!("{internal_name}.cs3"));
    }
    let slug = ascii_slug(internal_name);
    let key = stable_key(internal_name);
    let suffix = key.get(..HASH_PREFIX_LEN).unwrap_or(key.as_str());
    Ok(format!("{slug}_{suffix}.cs3"))
}

/// Ensures a stored `file_name` is strictly `<safe_stem>.cs3` (no traversal, no absolute path,
/// no Windows reserved device names, and only ASCII `[A-Za-z0-9_-]` in the stem).
pub(crate) fn validate_installed_file_name(file_name: &str) -> Result<(), RepoError> {
    if Path::new(file_name).is_absolute() {
        return Err(RepoError::UnsafeName(file_name.to_owned()));
    }
    if Path::new(file_name).components().any(|part| {
        matches!(
            part,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(RepoError::UnsafeName(file_name.to_owned()));
    }
    let stem = file_name
        .strip_suffix(".cs3")
        .ok_or_else(|| RepoError::UnsafeName(file_name.to_owned()))?;
    if !is_direct_safe_stem(stem) {
        return Err(RepoError::UnsafeName(file_name.to_owned()));
    }
    Ok(())
}

/// Joins `file_name` under `extensions_dir` only when the name is a safe installed plugin file.
pub fn plugin_path_in(extensions_dir: &Path, file_name: &str) -> Result<PathBuf, RepoError> {
    validate_installed_file_name(file_name)?;
    let joined = extensions_dir.join(file_name);
    if joined
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(RepoError::UnsafeName(file_name.to_owned()));
    }
    ensure_under_extensions(extensions_dir, &joined)?;
    Ok(joined)
}

fn reject_hostile_name(name: &str) -> Result<(), RepoError> {
    let trimmed = name.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
    {
        return Err(RepoError::UnsafeName(name.to_owned()));
    }
    Ok(())
}

fn is_direct_safe_stem(stem: &str) -> bool {
    !stem.is_empty()
        && stem.len() <= MAX_DIRECT_LEN
        && stem
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        && !is_windows_reserved(stem)
}

fn is_windows_reserved(stem: &str) -> bool {
    WINDOWS_RESERVED
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
}

fn ascii_slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len().min(MAX_SLUG_LEN));
    let mut last_sep = false;
    for ch in name.chars() {
        if out.len() >= MAX_SLUG_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '-' {
            out.push(ch);
            last_sep = false;
        } else if !last_sep && !out.is_empty() {
            out.push('_');
            last_sep = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() || is_windows_reserved(trimmed) {
        "plugin".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn ensure_under_extensions(extensions_dir: &Path, candidate: &Path) -> Result<(), RepoError> {
    let Ok(base) = extensions_dir.canonicalize() else {
        return Ok(());
    };
    match candidate.canonicalize() {
        Ok(canon) if canon.starts_with(&base) => Ok(()),
        Ok(canon) => Err(RepoError::Rejected {
            url: canon.display().to_string(),
            reason: "plugin path escapes the extensions folder".into(),
        }),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_identifier_unchanged() {
        assert_eq!(
            plugin_file_name("InternetArchiveProvider").expect("ok"),
            "InternetArchiveProvider.cs3"
        );
    }

    #[test]
    fn maps_punctuation_and_spaces_to_distinct_safe_filenames() {
        let plus = plugin_file_name("18+").expect("18+ maps safely");
        let space = plugin_file_name("18 ").expect("18 space maps safely");
        let cnc = plugin_file_name("CNC Verse").expect("CNC Verse maps safely");
        let live = plugin_file_name("LiveTGTV + M3U PLAYER").expect("LiveTGTV maps safely");

        assert!(plus.starts_with("18_") && plus.ends_with(".cs3"), "{plus}");
        assert_ne!(plus, space);
        assert!(cnc.starts_with("CNC_Verse_") && cnc.ends_with(".cs3"));
        assert!(live.starts_with("LiveTGTV_M3U_PLAYER_") && live.ends_with(".cs3"));

        for generated in [&plus, &space, &cnc, &live] {
            assert!(
                validate_installed_file_name(generated).is_ok(),
                "{generated}"
            );
        }
        assert_eq!(plus, plugin_file_name("18+").expect("deterministic"));
    }

    #[test]
    fn maps_windows_reserved_and_unicode_names_safely() {
        let con = plugin_file_name("CON").expect("CON maps safely");
        assert!(con.starts_with("plugin_") && con.ends_with(".cs3"), "{con}");
        assert!(validate_installed_file_name(&con).is_ok());
        assert!(validate_installed_file_name("CON.cs3").is_err());

        let arabic = plugin_file_name("عربي").expect("unicode maps safely");
        assert!(arabic.starts_with("plugin_") && arabic.ends_with(".cs3"));
        assert!(validate_installed_file_name(&arabic).is_ok());
    }

    #[test]
    fn rejects_path_traversal_separators_and_control_chars() {
        for bad in [
            "../evil", "a/b", "a\\b", "", "   ", ".", "..", "a\0b", "a\nb",
        ] {
            assert!(plugin_file_name(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn validate_installed_file_name_accepts_safe_cs3() {
        assert!(validate_installed_file_name("Good.cs3").is_ok());
    }

    #[test]
    fn validate_installed_file_name_rejects_traversal_special_chars_and_wrong_suffix() {
        for bad in [
            "../evil.cs3",
            "a/b.cs3",
            "Good",
            "Good.jar",
            "..cs3",
            "18+.cs3",
            "CNC Verse.cs3",
            "NUL.cs3",
        ] {
            assert!(
                validate_installed_file_name(bad).is_err(),
                "{bad} must be rejected"
            );
        }
    }

    #[test]
    fn plugin_path_in_joins_under_extensions_dir() {
        let dir = std::env::temp_dir().join("cs_safe_name_test");
        let path = plugin_path_in(&dir, "Good.cs3").expect("ok");
        assert_eq!(path, dir.join("Good.cs3"));
        assert!(plugin_path_in(&dir, "../Good.cs3").is_err());
    }
}
