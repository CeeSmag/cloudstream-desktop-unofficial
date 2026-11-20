use std::path::{Component, Path, PathBuf};

use crate::RepoError;

/// Turns a plugin's internal name into `<name>.cs3`, refusing anything that could escape the folder.
pub fn plugin_file_name(internal_name: &str) -> Result<String, RepoError> {
    let allowed =
        |character: char| character.is_ascii_alphanumeric() || matches!(character, '-' | '_');
    if internal_name.is_empty() || !internal_name.chars().all(allowed) {
        return Err(RepoError::UnsafeName(internal_name.to_owned()));
    }
    Ok(format!("{internal_name}.cs3"))
}

/// Ensures a stored `file_name` is exactly `<safe_stem>.cs3` (no traversal, no absolute path).
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
    let expected = plugin_file_name(stem)?;
    if expected != file_name {
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
    fn accepts_plain_identifier() {
        assert_eq!(
            plugin_file_name("InternetArchiveProvider").expect("ok"),
            "InternetArchiveProvider.cs3"
        );
    }

    #[test]
    fn rejects_path_traversal_and_separators() {
        for bad in ["../evil", "a/b", "a\\b", "", "name.cs3", "na me"] {
            assert!(plugin_file_name(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn validate_installed_file_name_accepts_safe_cs3() {
        assert!(validate_installed_file_name("Good.cs3").is_ok());
    }

    #[test]
    fn validate_installed_file_name_rejects_traversal_and_wrong_suffix() {
        for bad in ["../evil.cs3", "a/b.cs3", "Good", "Good.jar", "..cs3"] {
            assert!(
                validate_installed_file_name(bad).is_err(),
                "{bad} must be rejected"
            );
        }
    }

    #[test]
    fn plugin_path_in_joins_under_extensions_dir() {
        let dir = std::env::temp_dir().join(format!("cs_path_jail_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let path = plugin_path_in(&dir, "Good.cs3").expect("ok");
        assert_eq!(path, dir.join("Good.cs3"));
        assert!(plugin_path_in(&dir, "../Good.cs3").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
