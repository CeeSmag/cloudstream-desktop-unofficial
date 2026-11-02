use crate::RepoError;

/// Turns a plugin's internal name into `<name>.cs3`, refusing anything that could escape the folder.
pub(crate) fn plugin_file_name(internal_name: &str) -> Result<String, RepoError> {
    let allowed =
        |character: char| character.is_ascii_alphanumeric() || matches!(character, '-' | '_');
    if internal_name.is_empty() || !internal_name.chars().all(allowed) {
        return Err(RepoError::UnsafeName(internal_name.to_owned()));
    }
    Ok(format!("{internal_name}.cs3"))
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
}
