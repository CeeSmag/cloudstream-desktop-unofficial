/// One entry returned by a provider search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Display title shown in result lists.
    pub title: String,
    /// Provider-specific URL used to load the full details page.
    pub detail_url: String,
}

impl SearchResult {
    /// Builds a result, trimming surrounding whitespace from both fields.
    pub fn new(title: &str, detail_url: &str) -> Self {
        Self {
            title: title.trim().to_owned(),
            detail_url: detail_url.trim().to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_trims_whitespace_from_title_and_url() {
        let result = SearchResult::new("  Title ", " https://a.test/x ");
        assert_eq!(result.title, "Title");
        assert_eq!(result.detail_url, "https://a.test/x");
    }
}
