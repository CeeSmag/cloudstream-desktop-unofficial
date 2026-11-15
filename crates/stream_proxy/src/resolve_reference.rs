//! Turns a URL found in a playlist into an absolute one, the way RFC 3986 section 5 says (and so
//! the way the player would have done it when opening the stream directly: a query on the
//! playlist's own address is *not* handed on to the pieces it names).

use crate::url_parts::{has_scheme, split_url};

/// Removes `.` and `..` segments (RFC 3986 section 5.2.4); `..` never climbs above the root.
fn remove_dot_segments(path: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut segments = path.split('/').peekable();
    while let Some(segment) = segments.next() {
        let last = segments.peek().is_none();
        match segment {
            "." => {}
            ".." => {
                if kept.len() > 1 {
                    kept.pop();
                }
            }
            other => kept.push(other),
        }
        if last && matches!(segment, "." | "..") {
            kept.push("");
        }
    }
    let joined = kept.join("/");
    if joined.starts_with('/') {
        joined
    } else {
        format!("/{joined}")
    }
}

/// `base_path` up to and including its last `/`, followed by `reference_path`.
fn merge_paths(base_path: &str, reference_path: &str) -> String {
    match base_path.rfind('/') {
        Some(end) => format!("{}{reference_path}", base_path.get(..=end).unwrap_or("/")),
        None => format!("/{reference_path}"),
    }
}

/// The absolute URL that `reference` names when found in the document at `base`; `None` when
/// `base` is not an absolute URL. A reference with its own scheme is returned as it is: whether
/// that scheme may be fetched is for the caller to decide. Fragments are dropped.
pub(crate) fn resolve_reference(base: &str, reference: &str) -> Option<String> {
    let reference = reference.trim().split('#').next().unwrap_or("");
    let base = split_url(base)?;
    if has_scheme(reference) {
        return Some(reference.to_owned());
    }
    if let Some(rest) = reference.strip_prefix("//") {
        return Some(format!("{}://{rest}", base.scheme));
    }
    let (reference_path, reference_query) = match reference.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (reference, None),
    };
    let (path, query) = if reference_path.is_empty() {
        let keeps_base_query = reference_query.is_none() && reference.is_empty();
        let query = reference_query.or(keeps_base_query.then_some(base.query).flatten());
        (base.path.to_owned(), query)
    } else if reference_path.starts_with('/') {
        (remove_dot_segments(reference_path), reference_query)
    } else {
        let merged = merge_paths(base.path, reference_path);
        (remove_dot_segments(&merged), reference_query)
    };
    let query = query.map(|query| format!("?{query}")).unwrap_or_default();
    Some(format!("{}://{}{path}{query}", base.scheme, base.authority))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The base of the examples in RFC 3986 section 5.4.
    const BASE: &str = "http://a/b/c/d;p?q";

    fn resolved(reference: &str) -> String {
        resolve_reference(BASE, reference).expect("the base is absolute")
    }

    #[test]
    fn resolve_reference_rfc_normal_examples_return_the_rfc_answers() {
        let cases = [
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("", "http://a/b/c/d;p?q"),
            ("#s", "http://a/b/c/d;p?q"),
            ("g#s", "http://a/b/c/g"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
        ];
        for (reference, expected) in cases {
            assert_eq!(resolved(reference), expected, "reference {reference:?}");
        }
    }

    #[test]
    fn resolve_reference_rfc_abnormal_examples_return_the_rfc_answers() {
        let cases = [
            ("../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
            ("g?y/./x", "http://a/b/c/g?y/./x"),
            ("g?y/../x", "http://a/b/c/g?y/../x"),
        ];
        for (reference, expected) in cases {
            assert_eq!(resolved(reference), expected, "reference {reference:?}");
        }
    }

    #[test]
    fn resolve_reference_absolute_reference_is_returned_as_it_is() {
        assert_eq!(
            resolved("https://cdn.test/x/1.ts?t=9"),
            "https://cdn.test/x/1.ts?t=9"
        );
        assert_eq!(resolved("file:///etc/passwd"), "file:///etc/passwd");
    }

    #[test]
    fn resolve_reference_does_not_hand_the_playlists_query_to_its_segments() {
        let base = "https://cdn.test/v/master.m3u8?token=abc";
        assert_eq!(
            resolve_reference(base, "seg1.ts").as_deref(),
            Some("https://cdn.test/v/seg1.ts")
        );
        assert_eq!(
            resolve_reference(base, "seg1.ts?k=1").as_deref(),
            Some("https://cdn.test/v/seg1.ts?k=1")
        );
    }

    #[test]
    fn resolve_reference_scheme_relative_keeps_the_scheme_of_the_base() {
        assert_eq!(
            resolve_reference("https://a.test/x", "//b.test/y.ts").as_deref(),
            Some("https://b.test/y.ts")
        );
    }

    #[test]
    fn resolve_reference_base_without_path_still_resolves() {
        assert_eq!(
            resolve_reference("https://a.test", "x/y.ts").as_deref(),
            Some("https://a.test/x/y.ts")
        );
        assert_eq!(
            resolve_reference("https://a.test?z=1", "../y.ts").as_deref(),
            Some("https://a.test/y.ts")
        );
    }

    #[test]
    fn resolve_reference_trims_spaces_and_needs_an_absolute_base() {
        assert_eq!(resolved("  g  "), "http://a/b/c/g");
        assert_eq!(resolve_reference("not a url", "g"), None);
        assert_eq!(resolve_reference("/relative/base", "g"), None);
    }
}
