use std::time::UNIX_EPOCH;

use crate::pass_fixtures::{cookie, later, now, pass, run, url, Spec};

use super::*;

fn parts(cookies: Vec<PassCookie>) -> PassParts {
    PassParts {
        id: PassId::parse("p1").expect("id"),
        issued_for: url("https://www.site.test/watch"),
        user_agent: "Agent/1".into(),
        cookies,
        obtained_at: now(),
        run: run(),
    }
}

fn at(secs: u64) -> CookieExpiry {
    CookieExpiry::At(later(secs))
}

#[test]
fn a_pass_lives_as_long_as_its_earliest_applying_clearance() {
    let made = SavedPass::new(parts(vec![
        cookie("cf_clearance", "late", ".site.test", at(900)),
        cookie("cf_clearance", "early", "www.site.test", at(600)),
        cookie("cf_clearance", "elsewhere", "api.site.test", at(10)),
        cookie("cf_clearance", "", ".site.test", at(5)),
    ]))
    .expect("kept");
    assert_eq!(made.valid_until(), later(600));
    assert_eq!(made.clearance().value().expose(), "early");
    assert_eq!(
        made.scope(),
        &CookieDomain::HostOnly("www.site.test".into())
    );
}

#[test]
fn the_lifetime_cap_is_400_days_and_a_shorter_pass_keeps_its_own_end() {
    assert_eq!(MAX_PASS_LIFETIME, Duration::from_secs(400 * 86_400));
    let days = |count: u64| count * 86_400;
    let made = SavedPass::new(parts(vec![cookie(
        "cf_clearance",
        "v",
        ".site.test",
        at(days(399)),
    )]))
    .expect("kept");
    assert_eq!(made.valid_until(), later(days(399)));
}

#[test]
fn a_pass_never_outlives_the_cookie_lifetime_cap() {
    let ten_years = 10 * 365 * 24 * 60 * 60;
    let made = SavedPass::new(parts(vec![cookie(
        "cf_clearance",
        "v",
        ".site.test",
        at(ten_years),
    )]))
    .expect("kept");
    assert_eq!(made.valid_until(), now() + MAX_PASS_LIFETIME);
}

fn refuse(change: fn(&mut PassParts)) -> Option<PassRefusal> {
    let mut raw = parts(vec![cookie("cf_clearance", "v", ".site.test", at(600))]);
    change(&mut raw);
    SavedPass::new(raw).err()
}

#[test]
fn new_refuses_a_pass_without_a_usable_agent_or_page() {
    assert_eq!(
        refuse(|p| p.user_agent = " ".into()),
        Some(PassRefusal::BadAgent)
    );
    assert_eq!(
        refuse(|p| p.user_agent = "a\nb".into()),
        Some(PassRefusal::BadAgent)
    );
    assert_eq!(
        refuse(|p| p.issued_for = url("ftp://www.site.test/")),
        Some(PassRefusal::NotAWebPage)
    );
}

#[test]
fn new_refuses_a_pass_without_a_clearance_for_the_challenged_site() {
    assert_eq!(
        refuse(|p| p.cookies.clear()),
        Some(PassRefusal::NoClearance)
    );
    let session = |p: &mut PassParts| {
        p.cookies = vec![cookie(
            "cf_clearance",
            "v",
            ".site.test",
            CookieExpiry::Session,
        )];
    };
    assert_eq!(
        refuse(session),
        Some(PassRefusal::NoClearance),
        "a session clearance has no lifetime"
    );
    let elsewhere = |p: &mut PassParts| {
        p.cookies = vec![cookie("cf_clearance", "v", "other.test", at(600))];
    };
    assert_eq!(
        refuse(elsewhere),
        Some(PassRefusal::NoClearance),
        "another site's clearance"
    );
    let broad = |p: &mut PassParts| {
        p.issued_for = url("https://test/watch");
        p.cookies = vec![cookie("cf_clearance", "v", ".test", at(600))];
    };
    assert_eq!(refuse(broad), Some(PassRefusal::BroadDomain));
}

#[test]
fn new_drops_cookies_that_do_not_belong_to_the_challenged_address() {
    let made = SavedPass::new(parts(vec![
        cookie("cf_clearance", "v", ".site.test", at(600)),
        cookie("other_site", "x", "other.test", at(600)),
        cookie("sibling", "y", "api.site.test", at(600)),
        cookie("kept", "z", "www.site.test", CookieExpiry::Session),
    ]))
    .expect("kept");
    let names: Vec<&str> = made.parts().cookies.iter().map(PassCookie::name).collect();
    assert_eq!(names, ["cf_clearance", "kept"]);
}

#[test]
fn is_fresh_ends_one_margin_before_the_clearance() {
    let made = Spec::new("p1", "v").lasts(600).build();
    let edge = later(600) - SERVE_MARGIN;
    assert!(made.is_fresh(edge - Duration::from_millis(1)));
    assert!(!made.is_fresh(edge));
    assert!(!made.is_fresh(later(600)));
    if let Some(far) = UNIX_EPOCH.checked_add(Duration::from_secs(u64::MAX / 2)) {
        assert!(
            !made.is_fresh(far),
            "no overflow panic near the clock's end"
        );
    }
}

#[test]
fn covers_follows_the_clearance_domain() {
    let shared = pass("p1", "v");
    assert!(shared.covers(&url("https://api.site.test/x")));
    assert!(shared.covers(&url("https://site.test/")));
    assert!(
        !shared.covers(&url("http://www.site.test/")),
        "the clearance is Secure"
    );
    assert!(!shared.covers(&url("https://evilsite.test/")));
    let single = Spec::new("p2", "v").domain("www.site.test").build();
    assert!(single.covers(&url("https://www.site.test/other")));
    assert!(!single.covers(&url("https://api.site.test/")));
}

#[test]
fn served_for_hands_out_what_a_browser_would_send_now() {
    let made = SavedPass::new(parts(vec![
        cookie("cf_clearance", "v", ".site.test", at(900)),
        cookie("short", "s", ".site.test", at(100)),
        cookie("only_www", "w", "www.site.test", at(900)),
        cookie("session", "x", ".site.test", CookieExpiry::Session),
    ]))
    .expect("kept");
    let names = |served: ServedPass| -> Vec<String> {
        served
            .cookies
            .iter()
            .map(|cookie| cookie.name().to_owned())
            .collect()
    };
    let same_run = made.served_for(&url("https://www.site.test/"), now(), &run());
    assert_eq!(same_run.id.as_str(), "p1");
    assert_eq!(same_run.user_agent, "Agent/1");
    assert_eq!(same_run.valid_until, later(900));
    assert_eq!(
        names(same_run),
        ["cf_clearance", "short", "only_www", "session"]
    );
    let api_later = made.served_for(&url("https://api.site.test/"), later(100), &run());
    assert_eq!(
        names(api_later),
        ["cf_clearance", "session"],
        "short ended at 100 s"
    );
    let next_run = RunId::parse("r2").expect("run");
    let after_restart = made.served_for(&url("https://www.site.test/"), now(), &next_run);
    assert_eq!(names(after_restart), ["cf_clearance", "short", "only_www"]);
}

#[test]
fn refusals_explain_themselves() {
    for refusal in [
        PassRefusal::BadAgent,
        PassRefusal::NoClearance,
        PassRefusal::BroadDomain,
        PassRefusal::NotAWebPage,
    ] {
        assert!(!refusal.to_string().is_empty());
    }
    assert!(PassRefusal::BroadDomain.to_string().contains("top-level"));
}

#[test]
fn debug_of_a_pass_never_shows_its_cookie_values() {
    let printed = format!("{:?}", pass("p1", "clearance-secret-123"));
    assert!(!printed.contains("clearance-secret-123"), "{printed}");
}

#[test]
fn accessors_return_what_it_was_built_from() {
    let made = pass("p1", "v");
    assert_eq!(made.id().as_str(), "p1");
    assert_eq!(made.parts().run, run());
    assert_eq!(made.parts().obtained_at, now());
    assert!(made.valid_until() > UNIX_EPOCH);
}
