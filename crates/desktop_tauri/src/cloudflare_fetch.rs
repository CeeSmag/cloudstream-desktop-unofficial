//! Browser fetches: the sidecar asks for a request to be made *inside* a verification window.
//!
//! Cloudflare accepts the browser that passed its check but not the same cookies sent over the
//! sidecar's own HTTP client (it also looks at how the connection is made). So for such a site the
//! request is run with `fetch` in the window, and what the browser received goes back to the sidecar.
//!
//! The page cannot call the app, so the answer is read back the way Tauri allows: the script stores
//! its result in `window.__cs[id]` and the app asks for it every moment. The body travels as base64,
//! which the app never decodes (the sidecar does).

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};
use tauri::Webview;

/// How often the page is asked whether the request is done.
const POLL_PAUSE: Duration = Duration::from_millis(120);
/// How long one question to the page may take.
const EVAL_WAIT: Duration = Duration::from_secs(5);

/// A request the sidecar wants made in the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    /// Names the files of this request; letters, digits and `-` only.
    pub id: String,
    /// The site whose window makes the request.
    pub host: String,
    /// `GET`, `POST`, ...
    pub method: String,
    /// The address, on the window's own site.
    pub url: String,
    /// The `Referer` to send (a page may only set it through `fetch`'s own option).
    pub referer: Option<String>,
    /// Headers a page is allowed to set.
    pub headers: Vec<(String, String)>,
    /// The request body, base64.
    pub body_base64: Option<String>,
}

/// What came of a browser fetch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchOutcome {
    /// The browser got an answer.
    Done {
        /// HTTP status.
        status: u16,
        /// Address after redirects.
        url: String,
        /// Response headers.
        headers: Vec<(String, String)>,
        /// The body, base64, already decoded by the browser (not gzip or brotli any more).
        body_base64: String,
    },
    /// Nothing came back.
    Failed {
        /// Why.
        reason: String,
        /// The window does not exist, so the sidecar should have the site verified again.
        gone: bool,
    },
}

impl FetchOutcome {
    /// A failure that is not about a missing window.
    pub fn failed(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            gone: false,
        }
    }

    /// The window for the site does not exist.
    pub fn gone(reason: impl Into<String>) -> Self {
        Self::Failed {
            reason: reason.into(),
            gone: true,
        }
    }
}

/// Reads a `fetch-<id>.txt` file; None when a field is missing or names something unsafe.
pub fn parse_fetch_request(text: &str) -> Option<FetchRequest> {
    let one = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    let request = FetchRequest {
        id: one("id=")?,
        host: one("host=")?,
        method: one("method=")?.to_ascii_uppercase(),
        url: one("url=")?,
        referer: one("referer="),
        headers: text
            .lines()
            .filter_map(|line| line.strip_prefix("header="))
            .filter_map(|header| header.split_once(':'))
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .collect(),
        body_base64: one("body="),
    };
    let safe_id = request
        .id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-');
    let safe_method = request.method.chars().all(|c| c.is_ascii_alphabetic());
    let web_page = request.url.starts_with("https://") || request.url.starts_with("http://");
    (safe_id && safe_method && web_page).then_some(request)
}

/// Starts the request in the page; its result will appear in `window.__cs[id]`.
const START_HEAD: &str = "(function(o){window.__cs=window.__cs||{};var id=o.id;window.__cs[id]={pending:true};\
function b64(buf){var s='',b=new Uint8Array(buf),i=0,n=b.length;for(;i<n;i+=32768){s+=String.fromCharCode.apply(null,b.subarray(i,i+32768));}return btoa(s);}\
var init={method:o.method,headers:o.headers,credentials:'include',redirect:'follow'};\
if(o.referrer){init.referrer=o.referrer;}\
if(o.body){var bin=atob(o.body),u=new Uint8Array(bin.length);for(var k=0;k<bin.length;k++){u[k]=bin.charCodeAt(k);}init.body=u;}\
fetch(o.url,init).then(function(r){var hs=[];r.headers.forEach(function(v,k){hs.push([k,v]);});\
return r.arrayBuffer().then(function(buf){window.__cs[id]={ok:true,status:r.status,url:r.url,headers:hs,body:b64(buf)};});})\
.catch(function(e){window.__cs[id]={ok:false,reason:String(e)};});return 'started';})(";

/// The script that starts `request` in the page.
pub fn start_script(request: &FetchRequest) -> String {
    let headers: Map<String, Value> = request
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), Value::String(value.clone())))
        .collect();
    let options = json!({
        "id": request.id,
        "method": request.method,
        "url": request.url,
        "referrer": request.referer,
        "headers": headers,
        "body": request.body_base64,
    });
    format!("{START_HEAD}{options});")
}

/// The script that asks whether the request is done: `null` (not yet), `lost` (the page was
/// replaced, so the request is gone), or the result as JSON.
pub fn poll_script(id: &str) -> String {
    format!(
        "(function(){{var c=window.__cs;if(!c||!(('{id}') in c)){{return 'lost';}}var r=c['{id}'];\
if(r.pending){{return 'null';}}delete c['{id}'];return JSON.stringify(r);}})()"
    )
}

/// Reads what the page returned for `poll_script`; None means "not done yet".
pub fn parse_poll_result(raw: &str) -> Option<FetchOutcome> {
    // The page's string comes back JSON-encoded (quoted); the quotes are taken off first.
    let text = serde_json::from_str::<String>(raw).unwrap_or_else(|_| raw.to_owned());
    match text.trim() {
        "null" | "undefined" | "" => return None,
        "lost" => {
            return Some(FetchOutcome::failed(
                "the page in the window changed while the request ran",
            ))
        }
        _ => {}
    }
    let value: Value = serde_json::from_str(&text).ok()?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let reason = value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("the browser could not make the request");
        return Some(FetchOutcome::failed(reason));
    }
    Some(FetchOutcome::Done {
        status: value
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|s| u16::try_from(s).ok())
            .unwrap_or(200),
        url: value
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        headers: header_pairs(value.get("headers")),
        body_base64: value
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn header_pairs(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|pair| {
            let pair = pair.as_array()?;
            Some((
                pair.first()?.as_str()?.to_owned(),
                pair.get(1)?.as_str()?.to_owned(),
            ))
        })
        .collect()
}

/// The text of a `fetchresult-<id>.txt` file.
pub fn format_fetch_result(outcome: &FetchOutcome) -> String {
    match outcome {
        FetchOutcome::Failed { reason, gone } => {
            let gone = if *gone { "gone=true\n" } else { "" };
            format!("ok=false\n{gone}reason={}\n", reason.replace('\n', " "))
        }
        FetchOutcome::Done {
            status,
            url,
            headers,
            body_base64,
        } => {
            let mut text = format!("ok=true\nstatus={status}\nurl={url}\n");
            for (name, value) in headers {
                text.push_str(&format!("header={name}: {}\n", value.replace('\n', " ")));
            }
            text.push_str(&format!("body={body_base64}\n"));
            text
        }
    }
}

/// Runs `request` in `window` and waits for the browser's answer, at most `limit`.
pub fn run_fetch(window: &Webview, request: &FetchRequest, limit: Duration) -> FetchOutcome {
    if let Err(error) = window.eval(start_script(request)) {
        return FetchOutcome::failed(format!("cannot run the request in the window: {error}"));
    }
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if let Some(outcome) =
            ask_page(window, &poll_script(&request.id)).and_then(|raw| parse_poll_result(&raw))
        {
            return outcome;
        }
        std::thread::sleep(POLL_PAUSE);
    }
    FetchOutcome::failed("the browser did not answer in time")
}

/// Runs `script` in the page and returns what it returned.
fn ask_page(window: &Webview, script: &str) -> Option<String> {
    let (sender, receiver) = mpsc::channel();
    window
        .eval_with_callback(script, move |answer| {
            let _ = sender.send(answer);
        })
        .ok()?;
    receiver.recv_timeout(EVAL_WAIT).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> FetchRequest {
        parse_fetch_request("id=ab-1\nhost=site.test\nmethod=post\nurl=https://site.test/api?x=1\nreferer=https://site.test/\nheader=Accept: text/html\nheader=X-A: b: c\nbody=YT0x\n").expect("valid")
    }

    #[test]
    fn a_fetch_request_is_read_with_its_headers_and_body() {
        let request = request();
        assert_eq!(request.method, "POST");
        assert_eq!(request.referer.as_deref(), Some("https://site.test/"));
        assert_eq!(
            request.headers,
            vec![
                ("Accept".to_owned(), "text/html".to_owned()),
                ("X-A".to_owned(), "b: c".to_owned())
            ]
        );
        assert_eq!(request.body_base64.as_deref(), Some("YT0x"));
    }

    #[test]
    fn unsafe_requests_are_refused() {
        assert_eq!(
            parse_fetch_request("id=../x\nhost=h\nmethod=GET\nurl=https://h/\n"),
            None
        );
        assert_eq!(
            parse_fetch_request("id=a\nhost=h\nmethod=GET\nurl=file:///c:/x\n"),
            None
        );
        assert_eq!(
            parse_fetch_request("id=a\nhost=h\nmethod=G;ET\nurl=https://h/\n"),
            None
        );
        assert_eq!(parse_fetch_request("id=a\nhost=h\nurl=https://h/\n"), None);
    }

    #[test]
    fn the_start_script_carries_the_request_as_data_not_as_code() {
        let mut tricky = request();
        tricky.url = "https://site.test/a?q=\"';alert(1)//".to_owned();
        let script = start_script(&tricky);
        assert!(script.starts_with("(function(o)"));
        assert!(script.contains("fetch(o.url,init)"));
        assert!(
            script.contains("\\\"';alert(1)//"),
            "the quote is escaped inside the data: {script}"
        );
        assert!(script.contains("\"credentials\"") || script.contains("credentials:'include'"));
    }

    #[test]
    fn the_poll_result_is_not_done_lost_failed_or_done() {
        assert_eq!(parse_poll_result("\"null\""), None);
        assert_eq!(parse_poll_result("null"), None);
        assert!(matches!(
            parse_poll_result("\"lost\""),
            Some(FetchOutcome::Failed { gone: false, .. })
        ));
        let failed =
            parse_poll_result(r#""{\"ok\":false,\"reason\":\"TypeError: Failed to fetch\"}""#);
        assert_eq!(
            failed,
            Some(FetchOutcome::failed("TypeError: Failed to fetch"))
        );
        let done = parse_poll_result(
            r#""{\"ok\":true,\"status\":200,\"url\":\"https://s.test/a\",\"headers\":[[\"content-type\",\"text/html\"]],\"body\":\"aGk=\"}""#,
        );
        assert_eq!(
            done,
            Some(FetchOutcome::Done {
                status: 200,
                url: "https://s.test/a".to_owned(),
                headers: vec![("content-type".to_owned(), "text/html".to_owned())],
                body_base64: "aGk=".to_owned(),
            })
        );
    }

    #[test]
    fn results_are_written_for_the_sidecar() {
        let done = FetchOutcome::Done {
            status: 200,
            url: "u".into(),
            headers: vec![("a".into(), "b".into())],
            body_base64: "aGk=".into(),
        };
        assert_eq!(
            format_fetch_result(&done),
            "ok=true\nstatus=200\nurl=u\nheader=a: b\nbody=aGk=\n"
        );
        assert_eq!(
            format_fetch_result(&FetchOutcome::gone("no window")),
            "ok=false\ngone=true\nreason=no window\n"
        );
        assert_eq!(
            format_fetch_result(&FetchOutcome::failed("timeout")),
            "ok=false\nreason=timeout\n"
        );
    }

    #[test]
    fn the_poll_script_names_the_request() {
        let script = poll_script("ab-1");
        assert!(script.contains("c['ab-1']") && script.contains("'lost'"));
    }
}
