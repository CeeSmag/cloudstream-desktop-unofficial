use super::*;

#[test]
fn only_the_host_and_a_fingerprint_are_kept() {
    let redacted =
        redact_url("https://cdn.example.com:8443/hls/SECRET-TOKEN/index.m3u8?sig=abc123");
    assert!(redacted.starts_with("cdn.example.com#"), "{redacted}");
    for secret in ["SECRET-TOKEN", "abc123", "8443", "index.m3u8", "https"] {
        assert!(!redacted.contains(secret), "{redacted} leaks {secret}");
    }
}

#[test]
fn credentials_in_the_address_are_dropped() {
    let redacted = redact_url("https://user:hunter2@host.test/a");
    assert!(redacted.starts_with("host.test#"), "{redacted}");
    assert!(!redacted.contains("hunter2") && !redacted.contains("user"));
}

#[test]
fn two_links_of_one_host_get_different_fingerprints() {
    assert_ne!(
        redact_url("https://h.test/a?t=1"),
        redact_url("https://h.test/a?t=2")
    );
    assert_eq!(
        redact_url("https://h.test/a"),
        redact_url("https://h.test/a")
    );
}

#[test]
fn an_address_without_a_host_is_named_unknown_and_never_panics() {
    for odd in ["", "not a url", "https://", "://", "https:///path?x=1"] {
        let redacted = redact_url(odd);
        assert!(redacted.contains('#'), "{odd:?} -> {redacted}");
    }
    assert!(redact_url("").starts_with("unknown-host#"));
}

#[test]
fn an_ipv6_host_keeps_its_address_but_not_the_port() {
    assert!(redact_url("http://[::1]:9000/x").starts_with("::1#"));
}
