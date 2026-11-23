//! Passes for tests: built through the real constructors, at fixed times.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use url::Url;

use crate::cookie_value::CookieValue;
use crate::pass_cookie::{CookieExpiry, CookieParts, PassCookie};
use crate::pass_ids::{PassId, RunId};
use crate::saved_pass::{PassParts, SavedPass};

/// The tests' "now".
pub(crate) const NOW_SECS: u64 = 1_700_000_000;

pub(crate) fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(NOW_SECS)
}

pub(crate) fn later(secs: u64) -> SystemTime {
    now() + Duration::from_secs(secs)
}

pub(crate) fn url(text: &str) -> Url {
    Url::parse(text).expect("fixture URL")
}

pub(crate) fn run() -> RunId {
    RunId::parse("r1").expect("run")
}

pub(crate) fn cookie(name: &str, value: &str, domain: &str, expires: CookieExpiry) -> PassCookie {
    PassCookie::new(CookieParts {
        name: name.into(),
        value: CookieValue::new(value),
        domain: domain.into(),
        path: "/".into(),
        secure: true,
        http_only: true,
        expires,
    })
    .expect("fixture cookie")
}

/// How a fixture pass is made.
#[derive(Clone)]
pub(crate) struct Spec {
    pub(crate) id: String,
    pub(crate) host: String,
    pub(crate) domain: String,
    pub(crate) value: String,
    pub(crate) obtained: SystemTime,
    pub(crate) lasts_secs: u64,
}

impl Spec {
    pub(crate) fn new(id: &str, value: &str) -> Self {
        Self {
            id: id.to_owned(),
            host: "www.site.test".to_owned(),
            domain: ".site.test".to_owned(),
            value: value.to_owned(),
            obtained: now(),
            lasts_secs: 1_800,
        }
    }

    pub(crate) fn domain(mut self, domain: &str) -> Self {
        self.domain = domain.to_owned();
        self
    }

    pub(crate) fn host(mut self, host: &str) -> Self {
        self.host = host.to_owned();
        self
    }

    pub(crate) fn obtained(mut self, obtained: SystemTime) -> Self {
        self.obtained = obtained;
        self
    }

    pub(crate) fn lasts(mut self, secs: u64) -> Self {
        self.lasts_secs = secs;
        self
    }

    pub(crate) fn build(&self) -> SavedPass {
        let until = self.obtained + Duration::from_secs(self.lasts_secs);
        SavedPass::new(PassParts {
            id: PassId::parse(&self.id).expect("fixture id"),
            issued_for: url(&format!("https://{}/watch", self.host)),
            user_agent: "Agent/1".into(),
            cookies: vec![
                cookie(
                    "cf_clearance",
                    &self.value,
                    &self.domain,
                    CookieExpiry::At(until),
                ),
                cookie("__cf_bm", "bm", &self.domain, CookieExpiry::At(until)),
                cookie("site_session", "s", &self.domain, CookieExpiry::Session),
            ],
            obtained_at: self.obtained,
            run: run(),
        })
        .expect("fixture pass")
    }
}

/// A pass `id` with clearance `value` for `.site.test`, obtained now, lasting 30 minutes.
pub(crate) fn pass(id: &str, value: &str) -> SavedPass {
    Spec::new(id, value).build()
}

/// A temporary folder of its own.
pub(crate) fn folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "cs_passes_{name}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    dir
}
