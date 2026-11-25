//! Where the WebView2 setup comes from, and which addresses may serve it.

/// Microsoft's Evergreen bootstrapper (a small setup that downloads and installs the runtime).
pub const SETUP_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
/// Microsoft's page for installing WebView2 by hand.
pub const DOWNLOAD_PAGE: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";
/// The bootstrapper is about 2 MB; anything much bigger is not it.
pub const MAX_SETUP_BYTES: u64 = 16 * 1024 * 1024;
/// Redirects followed before giving up (the link redirects once or twice).
pub const MAX_HOPS: usize = 5;

/// `scheme` and `authority` of an absolute address.
fn split_address(url: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some((scheme, rest.get(..end)?))
}

/// `host` and `port` of an authority; refuses user names (`user@host`) and empty hosts.
fn split_authority(authority: &str) -> Option<(&str, Option<&str>)> {
    if authority.contains('@') {
        return None;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if port.bytes().all(|byte| byte.is_ascii_digit()) => (host, Some(port)),
        _ => (authority, None),
    };
    (!host.is_empty()).then_some((host, port))
}

/// The lower-case host of an absolute address, if it has a readable one.
pub fn host_of(url: &str) -> Option<String> {
    let (_, authority) = split_address(url)?;
    split_authority(authority).map(|(host, _)| host.to_ascii_lowercase())
}

/// `microsoft.com` or one of its subdomains.
pub fn is_microsoft_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "microsoft.com" || host.ends_with(".microsoft.com")
}

/// Whether the setup may be downloaded from `url`: HTTPS on the standard port, from Microsoft.
pub fn is_allowed_setup_address(url: &str) -> bool {
    let Some((scheme, authority)) = split_address(url) else {
        return false;
    };
    let Some((host, port)) = split_authority(authority) else {
        return false;
    };
    scheme.eq_ignore_ascii_case("https")
        && matches!(port, None | Some("443"))
        && is_microsoft_host(host)
}

/// The address a redirect from `current` to `location` leads to. Absolute (`https://…`),
/// scheme-relative (`//host/…`) and host-relative (`/path`) locations are followed; anything else
/// is refused (`None`).
pub fn next_hop(current: &str, location: &str) -> Option<String> {
    let location = location.trim();
    let (scheme, authority) = split_address(current)?;
    if split_address(location).is_some() {
        Some(location.to_owned())
    } else if let Some(rest) = location.strip_prefix("//") {
        Some(format!("{scheme}://{rest}"))
    } else if location.starts_with('/') {
        Some(format!("{scheme}://{authority}{location}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setup_link_itself_is_allowed() {
        assert!(is_allowed_setup_address(SETUP_URL));
        assert!(SETUP_URL.starts_with("https://"));
        assert!(is_allowed_setup_address(
            "https://msedge.sf.dl.delivery.mp.microsoft.com:443/x/MicrosoftEdgeWebview2Setup.exe"
        ));
        assert!(is_allowed_setup_address("HTTPS://Go.Microsoft.COM"));
    }

    #[test]
    fn other_hosts_schemes_ports_and_user_names_are_refused() {
        for url in [
            "http://go.microsoft.com/fwlink",
            "https://microsoft.com.example.org/setup.exe",
            "https://evilmicrosoft.com/setup.exe",
            "https://go.microsoft.com:8443/setup.exe",
            "https://go.microsoft.com@example.org/setup.exe",
            "https://example.org/?x=https://go.microsoft.com",
            "https:///setup.exe",
            "go.microsoft.com/setup.exe",
            "",
        ] {
            assert!(!is_allowed_setup_address(url), "{url}");
        }
    }

    #[test]
    fn hosts_are_read_lower_case_without_the_port() {
        assert_eq!(
            host_of("https://Go.Microsoft.com:443/a?b"),
            Some("go.microsoft.com".into())
        );
        assert_eq!(host_of("http://127.0.0.1:8080"), Some("127.0.0.1".into()));
        assert_eq!(host_of("https://user@host/"), None);
        assert_eq!(host_of("not an address"), None);
    }

    #[test]
    fn microsoft_hosts_are_the_domain_and_its_subdomains() {
        assert!(is_microsoft_host("microsoft.com"));
        assert!(is_microsoft_host("A.B.Microsoft.com"));
        assert!(!is_microsoft_host("microsoft.co"));
        assert!(!is_microsoft_host("notmicrosoft.com"));
        assert!(!is_microsoft_host("microsoft.com.evil"));
    }

    #[test]
    fn redirects_are_resolved_like_a_browser() {
        let current = "https://go.microsoft.com/fwlink/p/?LinkId=1";
        assert_eq!(
            next_hop(current, " https://cdn.microsoft.com/s.exe "),
            Some("https://cdn.microsoft.com/s.exe".into())
        );
        assert_eq!(
            next_hop(current, "//cdn.microsoft.com/s.exe"),
            Some("https://cdn.microsoft.com/s.exe".into())
        );
        assert_eq!(
            next_hop(current, "/other?x=1"),
            Some("https://go.microsoft.com/other?x=1".into())
        );
    }

    #[test]
    fn unreadable_redirects_are_refused() {
        assert_eq!(next_hop("https://go.microsoft.com/a/b", "c.exe"), None);
        assert_eq!(next_hop("not absolute", "/c.exe"), None);
    }

    // The bootstrapper is ~2 MB and redirects once or twice; the caps must leave room for that.
    const _: () =
        assert!(MAX_SETUP_BYTES >= 4 * 1024 * 1024 && MAX_SETUP_BYTES <= 64 * 1024 * 1024);
    const _: () = assert!(MAX_HOPS >= 2);
}
