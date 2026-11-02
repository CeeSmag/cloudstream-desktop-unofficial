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
            Some(failure) => Err(GatewayError::Remote {
                kind: failure.kind,
                message: failure.message.unwrap_or_default(),
            }),
            None => Ok(self.result.unwrap_or(Value::Null)),
        }
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
}
