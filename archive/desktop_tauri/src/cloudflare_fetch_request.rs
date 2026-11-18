//! The files of a browser fetch: `fetch-<id>.txt` (what the sidecar wants requested inside a
//! verification window) and `fetchresult-<id>.txt` (what the browser got). How the request is run
//! in the window is in `cloudflare_fetch.rs`.

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
        .all(|character| character.is_ascii_alphanumeric() || character == '-');
    let safe_method = request
        .method
        .chars()
        .all(|character| character.is_ascii_alphabetic());
    let web_page = request.url.starts_with("https://") || request.url.starts_with("http://");
    (safe_id && safe_method && web_page).then_some(request)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A valid POST request with a referer, two headers and a body.
    pub(crate) fn request() -> FetchRequest {
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
}
