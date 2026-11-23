//! The on-disk form of one scope (`<data>/cloudflare_passes/<scope>.json`), version 1.
//!
//! ```json
//! { "version": 1, "scope": ".site.test",
//!   "pass": { "id": "r1-1", "issued_for": "https://www.site.test/", "user_agent": "…",
//!             "cookies": [ { "name": "cf_clearance", "value": "…", "domain": ".site.test", "path": "/",
//!                            "secure": true, "http_only": true, "expires_ms": 1700000000000 } ],
//!             "obtained_ms": 1699990000000, "run": "r18b…-3a" },
//!   "rejected": [ { "value": "…", "until_ms": 1700000000000 } ] }
//! ```
//!
//! Reading checks everything again (the same constructors as a fresh pass), so a damaged or edited file is
//! refused, not trusted. Errors never quote the file.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::cookie_value::CookieValue;
use crate::exchange::epoch_millis;
use crate::issued_pass::{json_error, CookieRecord, PassWireError};
use crate::pass_cookie::CookieDomain;
use crate::pass_ids::{PassId, RunId};
use crate::pass_scope::{RejectedClearance, ScopeEntry, MAX_REJECTED_PER_SCOPE};
use crate::saved_pass::{PassParts, PassRefusal, SavedPass};

/// The schema this build writes and reads.
pub(crate) const SCOPE_FILE_VERSION: u32 = 1;

/// Why a scope file is refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ScopeFileError {
    #[error("the file is not a pass file: {0}")]
    Json(PassWireError),
    #[error("the file has schema version {0}, this build reads {SCOPE_FILE_VERSION}")]
    UnknownVersion(u32),
    #[error("the file's {0} is not valid")]
    BadField(&'static str),
    #[error("the file's pass: {0}")]
    Cookie(PassWireError),
    #[error("the file's pass cannot be kept: {0}")]
    Pass(PassRefusal),
    #[error("the file's pass belongs to another scope")]
    ScopeMismatch,
}

#[derive(Deserialize)]
struct Versioned {
    version: u32,
}

#[derive(Serialize, Deserialize)]
struct ScopeFile {
    version: u32,
    scope: String,
    #[serde(default)]
    pass: Option<StoredPass>,
    #[serde(default)]
    rejected: Vec<StoredRejection>,
}

#[derive(Serialize, Deserialize)]
struct StoredPass {
    id: String,
    issued_for: String,
    user_agent: String,
    cookies: Vec<CookieRecord>,
    obtained_ms: u64,
    run: String,
}

#[derive(Serialize, Deserialize)]
struct StoredRejection {
    value: String,
    until_ms: u64,
}

fn to_ms(time: SystemTime) -> Option<u64> {
    epoch_millis(time).and_then(|ms| u64::try_from(ms).ok())
}

fn from_ms(ms: u64) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_millis(ms))
}

/// The file's name for `scope`: its kind and its host, with anything but letters, digits, `.` and `-`
/// written as `_xx` (so IPv6 brackets and colons are safe on every file system and no two scopes collide).
pub(crate) fn scope_file_name(scope: &CookieDomain) -> String {
    let kind = match scope {
        CookieDomain::HostOnly(_) => "host",
        CookieDomain::Domain(_) => "domain",
    };
    let mut name = format!("{kind}-");
    for byte in scope.host().bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-') {
            name.push(char::from(byte));
        } else {
            name.push_str(&format!("_{byte:02x}"));
        }
    }
    name.push_str(".json");
    name
}

/// `entry` as file text. A time the format cannot hold leaves its item out (it could not be read back).
pub(crate) fn encode_scope(entry: &ScopeEntry) -> String {
    let file = ScopeFile {
        version: SCOPE_FILE_VERSION,
        scope: entry.scope.to_browser(),
        pass: entry.pass.as_ref().and_then(stored_pass),
        rejected: entry
            .rejected
            .iter()
            .filter_map(|rejected| {
                Some(StoredRejection {
                    value: rejected.value.expose().to_owned(),
                    until_ms: to_ms(rejected.until)?,
                })
            })
            .collect(),
    };
    // Plain strings, numbers and booleans always serialize; the fallback is refused when read back.
    serde_json::to_string(&file).unwrap_or_default()
}

fn stored_pass(pass: &SavedPass) -> Option<StoredPass> {
    let parts = pass.parts();
    Some(StoredPass {
        id: parts.id.as_str().to_owned(),
        issued_for: parts.issued_for.as_str().to_owned(),
        user_agent: parts.user_agent.clone(),
        cookies: parts
            .cookies
            .iter()
            .map(CookieRecord::from_cookie)
            .collect(),
        obtained_ms: to_ms(parts.obtained_at)?,
        run: parts.run.as_str().to_owned(),
    })
}

/// Reads file text back into a scope, checking every part.
pub(crate) fn decode_scope(text: &str) -> Result<ScopeEntry, ScopeFileError> {
    let parse_error = |error: serde_json::Error| ScopeFileError::Json(json_error(&error));
    let versioned: Versioned = serde_json::from_str(text).map_err(parse_error)?;
    if versioned.version != SCOPE_FILE_VERSION {
        return Err(ScopeFileError::UnknownVersion(versioned.version));
    }
    let file: ScopeFile = serde_json::from_str(text).map_err(parse_error)?;
    let scope = CookieDomain::parse(&file.scope).ok_or(ScopeFileError::BadField("scope"))?;
    let pass = file.pass.map(restore_pass).transpose()?;
    if pass.as_ref().is_some_and(|pass| *pass.scope() != scope) {
        return Err(ScopeFileError::ScopeMismatch);
    }
    let mut rejected = file
        .rejected
        .into_iter()
        .map(restore_rejection)
        .collect::<Result<Vec<_>, _>>()?;
    rejected.truncate(MAX_REJECTED_PER_SCOPE);
    Ok(ScopeEntry {
        scope,
        pass,
        rejected,
    })
}

fn restore_pass(stored: StoredPass) -> Result<SavedPass, ScopeFileError> {
    let cookies = stored
        .cookies
        .into_iter()
        .enumerate()
        .map(|(index, record)| record.into_cookie(index))
        .collect::<Result<Vec<_>, _>>()
        .map_err(ScopeFileError::Cookie)?;
    let parts = PassParts {
        id: PassId::parse(&stored.id).ok_or(ScopeFileError::BadField("pass id"))?,
        issued_for: Url::parse(&stored.issued_for)
            .map_err(|_| ScopeFileError::BadField("issued_for"))?,
        user_agent: stored.user_agent,
        cookies,
        obtained_at: from_ms(stored.obtained_ms).ok_or(ScopeFileError::BadField("obtained_ms"))?,
        run: RunId::parse(&stored.run).ok_or(ScopeFileError::BadField("run"))?,
    };
    SavedPass::new(parts).map_err(ScopeFileError::Pass)
}

fn restore_rejection(stored: StoredRejection) -> Result<RejectedClearance, ScopeFileError> {
    Ok(RejectedClearance {
        value: CookieValue::new(stored.value),
        until: from_ms(stored.until_ms).ok_or(ScopeFileError::BadField("until_ms"))?,
    })
}

#[cfg(test)]
#[path = "pass_file_tests.rs"]
mod tests;
