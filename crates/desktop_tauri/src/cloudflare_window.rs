//! The Cloudflare verification window: a browser window on the site, in which the viewer passes
//! Cloudflare's check. Once passed, the window's cookies and `User-Agent` go back to the sidecar, and
//! the window is kept (hidden) so that requests the sidecar's own HTTP client cannot make can be made
//! in it (see `cloudflare_fetch`).
//!
//! Everything is logged (`cloudflare-logs.txt`, and the main log at info level): each way of opening
//! the window and why it failed, what the window shows while waiting, and the result.

use std::path::Path;
use std::time::{Duration, Instant};

use app_services::ExtensionService;
use std::sync::Arc;

use tauri::{AppHandle, Manager, Url, Webview};

use crate::cloudflare_exchange::{Outcome, VerifyRequest};
use crate::cloudflare_fetch::{run_fetch, FetchOutcome, FetchRequest};
use crate::cloudflare_open::{open, reuse};
use crate::cloudflare_panel::{announce, PanelControl, Rect};

/// How often an open window is looked at.
const WINDOW_PAUSE: Duration = Duration::from_millis(500);
/// How often a waiting window says what it sees.
const REPORT_EVERY: Duration = Duration::from_secs(5);
/// How long the viewer gets to pass the check.
const VIEWER_TIME: Duration = Duration::from_secs(4 * 60);
/// While a window is open, calls waiting for it are kept alive this much further each time.
const HOLD_STEP: Duration = Duration::from_secs(15);
/// The cookie that proves the check was passed.
const CLEARANCE_COOKIE: &str = "cf_clearance";
/// After this many looks without the browser's `User-Agent`, the usual one is used.
const AGENT_PATIENCE: u32 = 12;
/// What Chrome-based browsers send, as a last resort; the sidecar's own default.
const FALLBACK_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/149.0.0.0 Safari/537.36";
/// Puts the browser's `User-Agent` into the page address (after the `#`), where the app can read it;
/// the page itself is not reloaded.
const AGENT_SCRIPT: &str = "try{var m='#cs-ua=';if(location.hash.indexOf(m)!==0){history.replaceState(null,'',location.pathname+location.search+m+encodeURIComponent(navigator.userAgent));}}catch(e){}";

/// How long one look at the site from inside the window may take.
const PROBE_TIME: Duration = Duration::from_secs(10);

/// The label of the window for `host`; one window per site.
pub fn label_for(host: &str) -> String {
    let safe: String = host
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("cf-{safe}")
}

/// Opens the check on the site and waits for the viewer. A check that passed is hidden and kept for
/// browser fetches; any other is closed.
pub fn verify(
    app: &AppHandle,
    helpers: (&ExtensionService, &Path),
    request: &VerifyRequest,
) -> Outcome {
    let (extensions, browser_profile) = helpers;
    let Ok(url) = request.url.parse::<Url>() else {
        return Outcome::Declined(format!("not a web address: {}", short(&request.url)));
    };
    let label = label_for(&request.host);
    let surface = match reuse(app, &label, &url) {
        Some(surface) => surface,
        None => match open(app, (&url, &label), browser_profile) {
            Ok(surface) => surface,
            Err(reason) => return Outcome::Declined(reason),
        },
    };
    let control = app.try_state::<Arc<PanelControl>>().map(|c| Arc::clone(&c));
    if let Some(control) = control.as_ref() {
        control.begin();
    }
    if surface.is_panel() {
        announce(app, &request.host, true);
    }
    let outcome = wait_for_clearance(surface.view(), &url, (extensions, control.as_deref()));
    if surface.is_panel() {
        announce(app, &request.host, false);
    }
    if matches!(outcome, Outcome::Passed { .. }) {
        surface.keep_hidden();
        tracing::info!(%label, panel = surface.is_panel(), "the check passed; it is hidden and kept for requests made in the browser");
    } else {
        surface.release();
        tracing::info!(%label, panel = surface.is_panel(), "the check ended without a pass; it is closed or parked");
    }
    outcome
}

/// What a waiting window has seen so far.
struct Watching {
    agent: Option<String>,
    looks: u32,
    probes: u32,
    last_report: Instant,
    placed: Option<Rect>,
}

/// Looks at the window until the check is passed, the window is closed, or the time is up.
fn wait_for_clearance(
    window: &Webview,
    url: &Url,
    helpers: (&ExtensionService, Option<&PanelControl>),
) -> Outcome {
    let (extensions, control) = helpers;
    let deadline = Instant::now() + VIEWER_TIME;
    let mut state = Watching {
        agent: None,
        looks: 0,
        probes: 0,
        last_report: Instant::now(),
        placed: None,
    };
    while Instant::now() < deadline {
        extensions.extend_call_deadline(HOLD_STEP);
        if control.is_some_and(PanelControl::is_cancelled) {
            return Outcome::Declined("the viewer cancelled the check".to_owned());
        }
        state.placed = control.and_then(PanelControl::placed);
        let Ok(current) = window.url() else {
            return Outcome::Declined("the window was closed".to_owned());
        };
        if state.agent.is_none() {
            state.agent = agent_from_fragment(current.fragment());
            let _ = window.eval(AGENT_SCRIPT);
        }
        state.looks += 1;
        let cookies = read_cookies(window, url);
        report(&current, &cookies, &mut state);
        let ready = state.agent.is_some() || state.looks >= AGENT_PATIENCE;
        let has_cookie = cookies.iter().any(|(name, _)| name == CLEARANCE_COOKIE);
        if ready && has_cookie && page_is_clear(window, url, &mut state) {
            let user_agent = state.agent.unwrap_or_else(|| FALLBACK_AGENT.to_owned());
            return Outcome::Passed {
                user_agent,
                cookies,
            };
        }
        std::thread::sleep(WINDOW_PAUSE);
    }
    Outcome::Declined("the check was not passed in time".to_owned())
}

/// Asks the site once from inside the window: is Cloudflare still checking? A `cf_clearance` cookie
/// can be left over from an earlier visit and no longer be accepted, so the cookie alone proves nothing.
fn page_is_clear(window: &Webview, url: &Url, state: &mut Watching) -> bool {
    state.probes += 1;
    let probe = FetchRequest {
        id: format!("probe-{}", state.probes),
        host: url.host_str().unwrap_or_default().to_owned(),
        method: "GET".to_owned(),
        url: url
            .as_str()
            .split('#')
            .next()
            .unwrap_or_default()
            .to_owned(),
        referer: None,
        headers: Vec::new(),
        body_base64: None,
    };
    match run_fetch(window, &probe, PROBE_TIME) {
        FetchOutcome::Done {
            status, headers, ..
        } => {
            let challenged = headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("cf-mitigated"));
            tracing::info!(
                status,
                challenged,
                probe = state.probes,
                "asked the site from inside the window"
            );
            !challenged
        }
        FetchOutcome::Failed { reason, .. } => {
            tracing::info!(%reason, probe = state.probes, "the window could not ask the site yet");
            false
        }
    }
}

/// The window's cookies for the site (those with a value); a failure is logged and counts as none.
fn read_cookies(window: &Webview, url: &Url) -> Vec<(String, String)> {
    match window.cookies_for_url(url.clone()) {
        Ok(cookies) => cookies
            .iter()
            .filter(|cookie| !cookie.value().is_empty())
            .map(|cookie| (cookie.name().to_owned(), cookie.value().to_owned()))
            .collect(),
        Err(error) => {
            tracing::warn!(error = ?error, "cannot read the verification window's cookies");
            Vec::new()
        }
    }
}

/// Every few seconds: what the window shows, so a stuck verification can be told from a slow one.
fn report(current: &Url, cookies: &[(String, String)], state: &mut Watching) {
    if state.last_report.elapsed() < REPORT_EVERY {
        return;
    }
    state.last_report = Instant::now();
    let names: Vec<&str> = cookies.iter().map(|(name, _)| name.as_str()).collect();
    tracing::info!(
        page = %format!("{}{}", current.host_str().unwrap_or("?"), current.path()),
        ?names,
        agent_known = state.agent.is_some(),
        looks = state.looks,
        placed = ?state.placed,
        "waiting for the Cloudflare check to be passed"
    );
}

/// A web address without its query, for the log.
pub fn short(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_owned()
}

/// The `User-Agent` that `AGENT_SCRIPT` left in the page address, if it has.
fn agent_from_fragment(fragment: Option<&str>) -> Option<String> {
    let text = fragment?.strip_prefix("cs-ua=")?;
    // The fragment is percent-encoded exactly like a query value, so a query parser decodes it.
    let parsed = Url::parse(&format!("http://agent.invalid/?ua={text}")).ok()?;
    parsed
        .query_pairs()
        .find(|(key, _)| key == "ua")
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_is_read_back_from_the_page_address() {
        let fragment = "cs-ua=Mozilla%2F5.0%20(Windows%20NT%2010.0%3B%20Win64)%20Chrome%2F149.0.0.0%20Safari%2F537.36";
        assert_eq!(
            agent_from_fragment(Some(fragment)).as_deref(),
            Some("Mozilla/5.0 (Windows NT 10.0; Win64) Chrome/149.0.0.0 Safari/537.36")
        );
    }

    #[test]
    fn a_plus_in_the_agent_stays_a_plus() {
        assert_eq!(
            agent_from_fragment(Some("cs-ua=A%2BB")).as_deref(),
            Some("A+B")
        );
    }

    #[test]
    fn no_agent_means_none() {
        assert_eq!(agent_from_fragment(None), None);
        assert_eq!(agent_from_fragment(Some("other=1")), None);
        assert_eq!(agent_from_fragment(Some("cs-ua=")), None);
    }

    #[test]
    fn the_page_script_leaves_the_agent_where_the_app_looks() {
        assert!(AGENT_SCRIPT.contains("#cs-ua="));
        assert!(AGENT_SCRIPT.contains("navigator.userAgent"));
    }

    #[test]
    fn a_logged_address_has_no_query_or_fragment() {
        assert_eq!(
            short("https://site.test/api?m=airing&token=secret#x"),
            "https://site.test/api"
        );
        assert_eq!(short("https://site.test/"), "https://site.test/");
    }

    #[test]
    fn each_site_has_its_own_window_label() {
        assert_eq!(label_for("animepahe.pw"), "cf-animepahe-pw");
        assert_ne!(label_for("a.test"), label_for("b.test"));
    }
}
