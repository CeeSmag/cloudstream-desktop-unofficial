use std::time::{Duration, UNIX_EPOCH};

use proptest::prelude::*;

use super::*;

fn parts(domain: &str, path: &str, secure: bool) -> CookieParts {
    CookieParts {
        name: "cf_clearance".into(),
        value: CookieValue::new("v"),
        domain: domain.into(),
        path: path.into(),
        secure,
        http_only: true,
        expires: CookieExpiry::Session,
    }
}

fn cookie(domain: &str, path: &str, secure: bool) -> PassCookie {
    PassCookie::new(parts(domain, path, secure)).expect("valid cookie")
}

fn url(text: &str) -> Url {
    Url::parse(text).expect("fixture URL")
}

#[test]
fn applies_to_matches_the_path_and_host_boundary() {
    let target = url("https://source.test/watch/title?ticket=one");
    assert!(cookie(".source.test", "/watch", true).applies_to(&target));
    assert!(!cookie("source.test", "/wat", true).applies_to(&target));
    assert!(!cookie("other.test", "/watch", false).applies_to(&target));
    assert!(!cookie(".test.invalid", "/", false).applies_to(&target));
    let plain = url("http://source.test/watch/title");
    assert!(!cookie("source.test", "/", true).applies_to(&plain));
    assert!(cookie("source.test", "/", false).applies_to(&plain));
}

#[test]
fn applies_to_a_domain_cookie_covers_subdomains_but_not_lookalikes() {
    let shared = cookie(".site.test", "/", true);
    assert!(shared.applies_to(&url("https://site.test/")));
    assert!(shared.applies_to(&url("https://api.site.test/x")));
    assert!(shared.applies_to(&url("https://a.b.site.test/x")));
    assert!(!shared.applies_to(&url("https://evilsite.test/")));
    assert!(!shared.applies_to(&url("https://site.test.evil/")));
}

#[test]
fn applies_to_a_host_only_cookie_stays_on_its_host() {
    let single = cookie("www.site.test", "/", true);
    assert!(single.applies_to(&url("https://www.site.test/")));
    assert!(single.applies_to(&url("https://WWW.Site.Test/")));
    assert!(!single.applies_to(&url("https://api.site.test/")));
    assert!(!single.applies_to(&url("https://site.test/")));
}

#[test]
fn applies_to_paths_follow_the_rfc_prefix_rule() {
    let target = |path: &str| url(&format!("https://site.test{path}"));
    let folder = cookie("site.test", "/docs/", false);
    assert!(folder.applies_to(&target("/docs/")));
    assert!(folder.applies_to(&target("/docs/a")));
    assert!(!folder.applies_to(&target("/docs")));
    let exact = cookie("site.test", "/docs", false);
    assert!(exact.applies_to(&target("/docs")));
    assert!(exact.applies_to(&target("/docs/a")));
    assert!(!exact.applies_to(&target("/docsx")));
    assert!(!exact.applies_to(&target("/")));
}

#[test]
fn applies_to_an_address_without_a_host_matches_nothing() {
    assert!(!cookie("site.test", "/", false).applies_to(&url("data:text/plain,x")));
}

#[test]
fn new_defaults_an_empty_path_to_the_root_and_lowercases_the_domain() {
    let made = cookie(".Site.TEST", "", false);
    assert_eq!(made.path(), "/");
    assert_eq!(made.domain(), &CookieDomain::Domain("site.test".into()));
    assert_eq!(made.domain().to_browser(), ".site.test");
    assert_eq!(made.domain().host(), "site.test");
    assert_eq!(
        cookie("Www.Site.Test", "/", false).domain().to_browser(),
        "www.site.test"
    );
}

#[test]
fn new_refuses_each_unsafe_field() {
    type Change = fn(&mut CookieParts);
    let cases: [(Change, CookieRefusal); 12] = [
        (|p| p.name.clear(), CookieRefusal::UnsafeName),
        (|p| p.name = "a=b".into(), CookieRefusal::UnsafeName),
        (|p| p.name = "a;b".into(), CookieRefusal::UnsafeName),
        (|p| p.name = "a b".into(), CookieRefusal::UnsafeName),
        (|p| p.name = "a\nb".into(), CookieRefusal::UnsafeName),
        (
            |p| p.value = CookieValue::new("a\nb"),
            CookieRefusal::UnsafeValue,
        ),
        (
            |p| p.value = CookieValue::new("a;b"),
            CookieRefusal::UnsafeValue,
        ),
        (|p| p.domain.clear(), CookieRefusal::BadDomain),
        (|p| p.domain = ".".into(), CookieRefusal::BadDomain),
        (|p| p.domain = "a/b".into(), CookieRefusal::BadDomain),
        (|p| p.domain = "a b.test".into(), CookieRefusal::BadDomain),
        (|p| p.path = "docs".into(), CookieRefusal::BadPath),
    ];
    for (index, (change, expected)) in cases.into_iter().enumerate() {
        let mut raw = parts("site.test", "/", false);
        change(&mut raw);
        assert_eq!(PassCookie::new(raw).err(), Some(expected), "case {index}");
    }
}

#[test]
fn new_accepts_values_browsers_accept() {
    for value in ["", "a=b", "x y", "\"quoted\"", "base64+/=="] {
        let mut raw = parts("site.test", "/", false);
        raw.value = CookieValue::new(value);
        assert!(PassCookie::new(raw).is_ok(), "{value:?}");
    }
    for domain in [
        "127.0.0.1",
        "[::1]",
        "xn--bcher-kva.test",
        "my_cdn.site.test",
    ] {
        assert!(
            PassCookie::new(parts(domain, "/", false)).is_ok(),
            "{domain}"
        );
    }
}

#[test]
fn accessors_return_the_checked_fields() {
    let mut raw = parts(".site.test", "/a", true);
    let until = UNIX_EPOCH + Duration::from_secs(1_000);
    raw.expires = CookieExpiry::At(until);
    raw.http_only = false;
    let made = PassCookie::new(raw).expect("valid");
    assert_eq!(made.name(), "cf_clearance");
    assert_eq!(made.value().expose(), "v");
    assert!(made.is_secure());
    assert!(!made.is_http_only());
    assert_eq!(made.expires(), CookieExpiry::At(until));
    assert!(cookie("site.test", "/", false).is_http_only());
}

#[test]
fn same_credential_compares_name_and_value_only() {
    let one = cookie(".site.test", "/", true);
    let other_scope = cookie("www.site.test", "/x", false);
    assert!(one.same_credential(&other_scope));
    let mut raw = parts("site.test", "/", false);
    raw.value = CookieValue::new("w");
    assert!(!one.same_credential(&PassCookie::new(raw).expect("valid")));
    let mut raw = parts("site.test", "/", false);
    raw.name = "other".into();
    assert!(!one.same_credential(&PassCookie::new(raw).expect("valid")));
}

#[test]
fn refusals_explain_themselves() {
    for refusal in [
        CookieRefusal::UnsafeName,
        CookieRefusal::UnsafeValue,
        CookieRefusal::BadDomain,
        CookieRefusal::BadPath,
    ] {
        assert!(refusal.to_string().starts_with("the cookie"), "{refusal}");
    }
}

#[test]
fn debug_of_a_cookie_hides_its_value() {
    let mut raw = parts("site.test", "/", false);
    raw.value = CookieValue::new("topsecretvalue");
    let printed = format!("{:?}", PassCookie::new(raw).expect("valid"));
    assert!(!printed.contains("topsecretvalue"), "{printed}");
}

fn label() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9]{0,6}"
}

proptest! {
    #[test]
    fn a_host_only_cookie_never_applies_to_another_host(own in label(), other in label(), tld in label()) {
        prop_assume!(own != other);
        let made = cookie(&format!("{own}.{tld}"), "/", false);
        let elsewhere = url(&format!("https://{other}.{tld}/"));
        let nested = url(&format!("https://{other}.{own}.{tld}/"));
        prop_assert!(!made.applies_to(&elsewhere));
        prop_assert!(!made.applies_to(&nested));
    }

    #[test]
    fn a_domain_cookie_applies_to_every_subdomain_and_no_suffix_lookalike(
        sub in label(), own in label(), tld in label(), glued in label(),
    ) {
        let made = cookie(&format!(".{own}.{tld}"), "/", false);
        let nested = url(&format!("https://{sub}.{own}.{tld}/"));
        prop_assert!(made.applies_to(&nested));
        let lookalike = url(&format!("https://{glued}{own}.{tld}/"));
        prop_assert!(!made.applies_to(&lookalike));
    }
}
