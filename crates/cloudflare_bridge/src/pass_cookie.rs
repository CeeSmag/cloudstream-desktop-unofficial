//! One browser cookie with the attributes that decide where it may be sent.
//!
//! The rule in [`PassCookie::applies_to`] is the browser's own (RFC 6265 §5.1.3 domain match, §5.1.4 path
//! match, `Secure`). It is the single place that decides which site a cookie belongs to: the verification
//! window uses it to pick the cookies of the challenged page, and the saved-pass store uses it to decide
//! which addresses a saved pass may answer for. Nothing guesses a cookie's reach from a host name.

use std::fmt;
use std::time::SystemTime;

use url::Url;

use crate::cookie_value::CookieValue;

/// Which hosts a cookie is sent to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CookieDomain {
    /// Set without a `Domain` attribute: only this exact host.
    HostOnly(String),
    /// Set with `Domain=site.test`: that host and every subdomain of it.
    Domain(String),
}

impl CookieDomain {
    /// Reads a browser's domain field: a leading dot marks a domain cookie (WebKit and WebView2 agree).
    pub fn parse(raw: &str) -> Option<Self> {
        let lowered = raw.trim().to_ascii_lowercase();
        let (is_domain, host) = match lowered.strip_prefix('.') {
            Some(rest) => (true, rest.to_owned()),
            None => (false, lowered),
        };
        let valid = !host.is_empty()
            && host.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']')
            });
        if !valid {
            return None;
        }
        Some(if is_domain {
            Self::Domain(host)
        } else {
            Self::HostOnly(host)
        })
    }

    /// The host the attribute names, without the leading dot.
    pub fn host(&self) -> &str {
        match self {
            Self::HostOnly(host) | Self::Domain(host) => host,
        }
    }

    /// The browser's field again: a domain cookie with its leading dot.
    pub fn to_browser(&self) -> String {
        match self {
            Self::HostOnly(host) => host.clone(),
            Self::Domain(host) => format!(".{host}"),
        }
    }

    /// True when a request to `host` carries a cookie of this domain.
    pub fn covers(&self, host: &str) -> bool {
        let host = host.to_ascii_lowercase();
        match self {
            Self::HostOnly(own) => host == *own,
            Self::Domain(own) => {
                host == *own
                    || host
                        .strip_suffix(own.as_str())
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
        }
    }
}

/// When a cookie ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieExpiry {
    /// No expiry: the browser forgets it when it closes.
    Session,
    /// The cookie's `Expires` / `Max-Age`.
    At(SystemTime),
}

/// The raw fields a browser reports, before they are checked.
#[derive(Debug, Clone)]
pub struct CookieParts {
    /// The cookie's name.
    pub name: String,
    /// Its value.
    pub value: CookieValue,
    /// The browser's domain field (a leading dot for a domain cookie).
    pub domain: String,
    /// The `Path` attribute.
    pub path: String,
    /// `Secure`: only sent over HTTPS.
    pub secure: bool,
    /// `HttpOnly`: hidden from scripts.
    pub http_only: bool,
    /// When it ends.
    pub expires: CookieExpiry,
}

/// Why a browser's cookie cannot be handed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieRefusal {
    /// The name is empty or holds `=`, `;`, white space or a control character.
    UnsafeName,
    /// The value holds `;`, a line break or another control character (it would break the result file).
    UnsafeValue,
    /// The domain field is empty or is not a host name.
    BadDomain,
    /// The path does not start with `/`.
    BadPath,
}

impl fmt::Display for CookieRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::UnsafeName => "the cookie name is not a valid token",
            Self::UnsafeValue => "the cookie value holds a separator or control character",
            Self::BadDomain => "the cookie domain is not a host name",
            Self::BadPath => "the cookie path does not start with '/'",
        })
    }
}

impl std::error::Error for CookieRefusal {}

/// A checked browser cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassCookie {
    name: String,
    value: CookieValue,
    domain: CookieDomain,
    path: String,
    secure: bool,
    http_only: bool,
    expires: CookieExpiry,
}

impl PassCookie {
    /// Checks `parts`; an empty path is the root, as browsers treat it.
    pub fn new(parts: CookieParts) -> Result<Self, CookieRefusal> {
        if !is_token(&parts.name) {
            return Err(CookieRefusal::UnsafeName);
        }
        if !is_safe_value(parts.value.expose()) {
            return Err(CookieRefusal::UnsafeValue);
        }
        let domain = CookieDomain::parse(&parts.domain).ok_or(CookieRefusal::BadDomain)?;
        let path = if parts.path.is_empty() {
            "/".to_owned()
        } else {
            parts.path
        };
        if !path.starts_with('/') {
            return Err(CookieRefusal::BadPath);
        }
        Ok(Self {
            name: parts.name,
            value: parts.value,
            domain,
            path,
            secure: parts.secure,
            http_only: parts.http_only,
            expires: parts.expires,
        })
    }

    /// The cookie's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Its value (redacted in `Debug`).
    pub fn value(&self) -> &CookieValue {
        &self.value
    }

    /// The hosts it is sent to.
    pub fn domain(&self) -> &CookieDomain {
        &self.domain
    }

    /// Its `Path`.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// True when only sent over HTTPS.
    pub fn is_secure(&self) -> bool {
        self.secure
    }

    /// True when hidden from scripts.
    pub fn is_http_only(&self) -> bool {
        self.http_only
    }

    /// When it ends.
    pub fn expires(&self) -> CookieExpiry {
        self.expires
    }

    /// True when a browser would send this cookie with a request to `url`.
    pub fn applies_to(&self, url: &Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        self.domain.covers(host)
            && path_matches(url.path(), &self.path)
            && (!self.secure || url.scheme() == "https")
    }

    /// True when the same name and value (what the site sees), whatever the attributes.
    pub fn same_credential(&self, other: &Self) -> bool {
        self.name == other.name && self.value == other.value
    }
}

/// RFC 6265 §5.1.4: the cookie path is the request path, or a prefix of it ending at a `/` boundary.
fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || request_path
            .strip_prefix(cookie_path)
            .is_some_and(|tail| cookie_path.ends_with('/') || tail.starts_with('/'))
}

fn is_token(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_graphic() && !matches!(c, '=' | ';'))
}

fn is_safe_value(value: &str) -> bool {
    value.chars().all(|c| !c.is_control() && c != ';')
}

#[cfg(test)]
#[path = "pass_cookie_tests.rs"]
mod tests;
