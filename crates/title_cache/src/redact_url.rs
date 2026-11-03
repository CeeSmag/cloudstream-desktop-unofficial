//! Makes a stream address safe to put in a log: stream links often carry tokens in the path or the
//! query, so only the host and a short fingerprint are kept. The fingerprint tells two different
//! links of one host apart.

use core_model::stable_key;

const FINGERPRINT_CHARS: usize = 8;

/// `host#fingerprint` for `url`; never the path, the query, the credentials or the port.
pub fn redact_url(url: &str) -> String {
    let fingerprint: String = stable_key(url).chars().take(FINGERPRINT_CHARS).collect();
    format!("{}#{fingerprint}", host_of(url))
}

fn host_of(url: &str) -> &str {
    let after_scheme = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    let host_and_port = authority.rsplit('@').next().unwrap_or_default();
    let host = match host_and_port.strip_prefix('[') {
        Some(inside) => inside.split(']').next().unwrap_or_default(),
        None => host_and_port.split(':').next().unwrap_or_default(),
    };
    let looks_like_a_host = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'));
    if looks_like_a_host {
        host
    } else {
        "unknown-host"
    }
}

#[cfg(test)]
#[path = "redact_url_tests.rs"]
mod tests;
