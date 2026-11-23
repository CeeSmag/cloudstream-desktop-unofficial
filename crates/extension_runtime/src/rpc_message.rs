use extension_api::GatewayError;
use serde::Deserialize;
use serde_json::{json, Value};

/// Encodes one request as a single JSON line (without the trailing newline).
pub(crate) fn encode_request(id: u64, method: &str, params: &Value) -> String {
    json!({ "id": id, "method": method, "params": params }).to_string()
}

/// Failure details sent by the runtime.
#[derive(Debug, Deserialize)]
pub(crate) struct RemoteFailure {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    violations: Vec<String>,
}

/// One reply line, before it is matched to a request.
#[derive(Debug, Deserialize)]
pub(crate) struct ReplyEnvelope {
    pub(crate) id: Option<u64>,
    result: Option<Value>,
    error: Option<RemoteFailure>,
}

/// Parses one reply line.
pub(crate) fn parse_reply(line: &str) -> Result<ReplyEnvelope, GatewayError> {
    serde_json::from_str(line).map_err(|source| {
        GatewayError::Transport(format!(
            "unreadable line from runtime ({source}): {}",
            line.chars().take(120).collect::<String>()
        ))
    })
}

impl ReplyEnvelope {
    /// Converts the reply into the call's result value or a typed error.
    pub(crate) fn into_value(self) -> Result<Value, GatewayError> {
        match self.error {
            Some(failure) => Err(decode_failure(failure)),
            None => Ok(self.result.unwrap_or(Value::Null)),
        }
    }
}

fn decode_failure(failure: RemoteFailure) -> GatewayError {
    let message = failure.message.unwrap_or_default();
    match failure.kind.as_str() {
        "PluginSecurityBlockedException" | "RequiresPermissionException" => {
            GatewayError::SecurityBlocked {
                bypassable: true,
                violations: failure.violations,
                message,
            }
        }
        "PluginSecurityDeniedException" => GatewayError::SecurityBlocked {
            bypassable: false,
            violations: failure.violations,
            message,
        },
        _ => GatewayError::Remote {
            kind: failure.kind,
            message,
        },
    }
}

/// True when two `load_extension` params name the same plugin path (slash/case normalized).
pub(crate) fn same_extension_load(left: &Value, right: &Value) -> bool {
    let path = |value: &Value| {
        value
            .get("path")
            .and_then(Value::as_str)
            .map(|text| text.replace('\\', "/").to_ascii_lowercase())
    };
    match (path(left), path(right)) {
        (Some(a), Some(b)) => a == b,
        _ => left == right,
    }
}

/// RPCs tied to in-memory state of one running JVM; retrying them on a fresh JVM always fails.
pub(crate) fn is_per_process_method(method: &str) -> bool {
    matches!(
        method,
        "poll_link_search" | "cancel_link_search" | "take_added_repositories"
    )
}

/// Error returned when a per-process RPC arrives with no live sidecar (avoiding a pointless restart).
pub(crate) fn missing_sidecar_error(method: &str) -> Option<GatewayError> {
    match method {
        "take_added_repositories" => Some(GatewayError::Unavailable(
            "no sidecar running for take_added".to_owned(),
        )),
        "poll_link_search" | "cancel_link_search" => Some(GatewayError::Transport(
            "sidecar exited before link search finished".to_owned(),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_request_produces_one_line_with_id_method_and_params() {
        let line = encode_request(7, "search", &json!({"query": "moon"}));
        assert!(!line.contains('\n'));
        let parsed: Value = serde_json::from_str(&line).expect("valid JSON");
        assert_eq!(parsed["id"], 7);
        assert_eq!(parsed["method"], "search");
        assert_eq!(parsed["params"]["query"], "moon");
    }

    #[test]
    fn parse_reply_with_result_yields_value() {
        let reply = parse_reply(r#"{"id":3,"result":{"ok":true}}"#).expect("valid reply");
        assert_eq!(reply.id, Some(3));
        assert_eq!(reply.into_value().expect("result"), json!({"ok": true}));
    }

    #[test]
    fn parse_reply_with_null_result_yields_null() {
        let reply = parse_reply(r#"{"id":3,"result":null}"#).expect("valid reply");
        assert_eq!(reply.into_value().expect("null result"), Value::Null);
    }

    #[test]
    fn parse_reply_with_error_yields_remote_error() {
        let reply =
            parse_reply(r#"{"id":3,"error":{"type":"IllegalStateException","message":"bad"}}"#)
                .expect("valid reply");
        assert_eq!(
            reply.into_value(),
            Err(GatewayError::Remote {
                kind: "IllegalStateException".to_owned(),
                message: "bad".to_owned(),
            })
        );
    }

    #[test]
    fn parse_reply_rejects_non_json_noise() {
        assert!(matches!(
            parse_reply("[main] INFO something"),
            Err(GatewayError::Transport(_))
        ));
    }

    #[test]
    fn parse_reply_decodes_typed_security_errors() {
        let verifier = parse_reply(
            r#"{"id":4,"error":{"type":"PluginSecurityBlockedException","message":"blocked","violations":["Disallowed class 'java.lang.ClassLoader'"]}}"#,
        )
        .expect("valid reply");
        assert_eq!(
            verifier.into_value(),
            Err(GatewayError::SecurityBlocked {
                bypassable: true,
                violations: vec!["Disallowed class 'java.lang.ClassLoader'".to_owned()],
                message: "blocked".to_owned(),
            })
        );

        let perm = parse_reply(
            r#"{"id":5,"error":{"type":"RequiresPermissionException","message":"raw socket","violations":["raw socket"]}}"#,
        )
        .expect("valid reply");
        assert!(matches!(
            perm.into_value(),
            Err(GatewayError::SecurityBlocked {
                bypassable: true,
                ..
            })
        ));
    }

    #[test]
    fn parse_reply_distinguishes_hard_denial_and_path_jail_security_exception() {
        let hard = parse_reply(
            r#"{"id":6,"error":{"type":"PluginSecurityDeniedException","message":"denied","violations":["Disallowed class 'java.lang.ProcessBuilder'"]}}"#,
        )
        .expect("valid reply");
        assert_eq!(
            hard.into_value(),
            Err(GatewayError::SecurityBlocked {
                bypassable: false,
                violations: vec!["Disallowed class 'java.lang.ProcessBuilder'".to_owned()],
                message: "denied".to_owned(),
            })
        );

        // ExtensionPathJail throws plain SecurityException: must NOT become SecurityBlocked.
        let jail = parse_reply(
            r#"{"id":7,"error":{"type":"SecurityException","message":"cloudstream.data.dir is not set"}}"#,
        )
        .expect("valid reply");
        assert_eq!(
            jail.into_value(),
            Err(GatewayError::Remote {
                kind: "SecurityException".to_owned(),
                message: "cloudstream.data.dir is not set".to_owned(),
            })
        );
    }
}
