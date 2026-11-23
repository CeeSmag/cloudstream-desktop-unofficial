//! A pass as the verification helper reports it to the app, and its JSON form.
//!
//! The helper answers the sidecar itself (`result-<id>.txt`, which the sidecar deletes once read), so the app
//! learns the pass only from the helper's private stdout: `passed\t<host>\t<id>\t<json>`. The same cookie
//! record is the on-disk form of a saved pass (`disk_pass_book`). serde_json escapes tabs and line breaks, so
//! a payload is always one field of one line.

use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::cookie_value::CookieValue;
use crate::exchange::epoch_millis;
use crate::pass_cookie::{CookieExpiry, CookieParts, CookieRefusal, PassCookie};

/// The largest payload read; a real pass is a few kilobytes.
pub(crate) const MAX_PASS_PAYLOAD_BYTES: usize = 64 * 1024;

/// The most cookies one pass may hold; a site sets a handful.
pub(crate) const MAX_PASS_COOKIES: usize = 64;

/// What passed the check: the browser's identity and the cookies it holds for the challenged address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedPass {
    /// The `User-Agent` the pass was issued to; Cloudflare ties the pass to it.
    pub user_agent: String,
    /// The browser's cookies for the challenged address.
    pub cookies: Vec<PassCookie>,
}

/// Why a pass payload cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum PassWireError {
    /// Larger than [`MAX_PASS_PAYLOAD_BYTES`].
    #[error("the pass payload has {bytes} bytes, more than {MAX_PASS_PAYLOAD_BYTES}")]
    TooLarge {
        /// Its size.
        bytes: usize,
    },
    /// Not the expected JSON. Only where and what kind: serde's own message can quote the input, which
    /// may be a cookie value.
    #[error("the pass payload is not a pass ({kind} error at line {line}, column {column})")]
    Json {
        /// `syntax`, `data`, `eof` or `io`.
        kind: &'static str,
        /// Where it went wrong.
        line: usize,
        /// Where it went wrong.
        column: usize,
    },
    /// More than [`MAX_PASS_COOKIES`] cookies.
    #[error("the pass holds {count} cookies, more than {MAX_PASS_COOKIES}")]
    TooManyCookies {
        /// How many it holds.
        count: usize,
    },
    /// A cookie that cannot be handed on.
    #[error("cookie {index} of the pass: {refusal}")]
    Cookie {
        /// Its position in the pass.
        index: usize,
        /// What is wrong with it.
        refusal: CookieRefusal,
    },
    /// A cookie whose expiry the clock cannot hold.
    #[error("cookie {index} of the pass has an expiry the clock cannot hold")]
    BadExpiry {
        /// Its position in the pass.
        index: usize,
    },
    /// No browser identity.
    #[error("the pass names no user agent")]
    EmptyAgent,
}

/// One cookie as JSON. `value` is the credential: this type is never logged.
#[derive(Serialize, Deserialize)]
pub(crate) struct CookieRecord {
    name: String,
    value: String,
    domain: String,
    path: String,
    secure: bool,
    http_only: bool,
    /// Milliseconds since 1970; absent for a session cookie.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_ms: Option<u64>,
}

/// A pass as JSON.
#[derive(Serialize, Deserialize)]
pub(crate) struct IssuedPassRecord {
    user_agent: String,
    cookies: Vec<CookieRecord>,
}

impl CookieRecord {
    pub(crate) fn from_cookie(cookie: &PassCookie) -> Self {
        let expires_ms = match cookie.expires() {
            CookieExpiry::Session => None,
            CookieExpiry::At(time) => epoch_millis(time).and_then(|ms| u64::try_from(ms).ok()),
        };
        Self {
            name: cookie.name().to_owned(),
            value: cookie.value().expose().to_owned(),
            domain: cookie.domain().to_browser(),
            path: cookie.path().to_owned(),
            secure: cookie.is_secure(),
            http_only: cookie.is_http_only(),
            expires_ms,
        }
    }

    pub(crate) fn into_cookie(self, index: usize) -> Result<PassCookie, PassWireError> {
        let expires = match self.expires_ms {
            None => CookieExpiry::Session,
            Some(ms) => UNIX_EPOCH
                .checked_add(Duration::from_millis(ms))
                .map(CookieExpiry::At)
                .ok_or(PassWireError::BadExpiry { index })?,
        };
        PassCookie::new(CookieParts {
            name: self.name,
            value: CookieValue::new(self.value),
            domain: self.domain,
            path: self.path,
            secure: self.secure,
            http_only: self.http_only,
            expires,
        })
        .map_err(|refusal| PassWireError::Cookie { index, refusal })
    }
}

impl IssuedPassRecord {
    pub(crate) fn from_pass(pass: &IssuedPass) -> Self {
        Self {
            user_agent: pass.user_agent.clone(),
            cookies: pass.cookies.iter().map(CookieRecord::from_cookie).collect(),
        }
    }

    pub(crate) fn into_pass(self) -> Result<IssuedPass, PassWireError> {
        if self.user_agent.trim().is_empty() {
            return Err(PassWireError::EmptyAgent);
        }
        if self.cookies.len() > MAX_PASS_COOKIES {
            return Err(PassWireError::TooManyCookies {
                count: self.cookies.len(),
            });
        }
        let cookies = self
            .cookies
            .into_iter()
            .enumerate()
            .map(|(index, record)| record.into_cookie(index))
            .collect::<Result<_, _>>()?;
        Ok(IssuedPass {
            user_agent: self.user_agent,
            cookies,
        })
    }
}

/// The pass as one JSON field (no tab, no line break).
pub(crate) fn encode_pass(pass: &IssuedPass) -> String {
    // Serializing plain strings, booleans and numbers into a String cannot fail; an empty object would
    // be refused by `decode_pass`, so the fallback is never mistaken for a pass.
    serde_json::to_string(&IssuedPassRecord::from_pass(pass)).unwrap_or_else(|_| "{}".to_owned())
}

/// Reads a pass field; every cookie is checked as the browser's cookies are.
pub fn decode_pass(text: &str) -> Result<IssuedPass, PassWireError> {
    if text.len() > MAX_PASS_PAYLOAD_BYTES {
        return Err(PassWireError::TooLarge { bytes: text.len() });
    }
    serde_json::from_str::<IssuedPassRecord>(text)
        .map_err(|error| json_error(&error))?
        .into_pass()
}

/// `error` without its message (which can quote the input).
pub(crate) fn json_error(error: &serde_json::Error) -> PassWireError {
    let kind = match error.classify() {
        serde_json::error::Category::Io => "io",
        serde_json::error::Category::Syntax => "syntax",
        serde_json::error::Category::Data => "data",
        serde_json::error::Category::Eof => "eof",
    };
    PassWireError::Json {
        kind,
        line: error.line(),
        column: error.column(),
    }
}

#[cfg(test)]
#[path = "issued_pass_tests.rs"]
mod tests;
