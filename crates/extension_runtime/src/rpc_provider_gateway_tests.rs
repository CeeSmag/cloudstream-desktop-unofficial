use std::sync::Mutex;

use super::*;

/// Returns canned values and records every call.
struct ScriptedTransport {
    reply: Result<Value, GatewayError>,
    calls: Mutex<Vec<(String, Value)>>,
}

impl ScriptedTransport {
    fn replying(reply: Result<Value, GatewayError>) -> Self {
        Self {
            reply,
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl RpcTransport for ScriptedTransport {
    fn call(&self, method: &str, params: Value, _timeout: Duration) -> Result<Value, GatewayError> {
        self.calls
            .lock()
            .expect("lock")
            .push((method.to_owned(), params));
        self.reply.clone()
    }
}

#[test]
fn search_sends_provider_query_and_page_and_decodes_items() {
    let reply = json!({"items":[{"name":"Apollo","url":"u"}],"hasNext":true});
    let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(reply)));

    let page = gateway
        .search("Archive", "moon", 2)
        .expect("search succeeds");

    assert_eq!(page.items[0].name, "Apollo");
    assert!(page.has_next);
    let calls = gateway.transport.calls.lock().expect("lock");
    assert_eq!(calls[0].0, "search");
    assert_eq!(
        calls[0].1,
        json!({"provider":"Archive","query":"moon","page":2})
    );
}

#[test]
fn a_call_is_named_after_its_provider_or_the_extension_file() {
    assert_eq!(call_target(&json!({"provider":"CineStream"})), "CineStream");
    assert_eq!(
        call_target(&json!({"path": r"C:\data\ext\Mega.cs3"})),
        "Mega.cs3"
    );
    assert_eq!(call_target(&json!({})), "");
}

#[test]
fn take_added_repositories_decodes_the_runtime_reply() {
    let reply = json!([{"name":"Repo","url":"https://r/repo.json","iconUrl":null}]);
    let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(reply)));
    let found = gateway.take_added_repositories().expect("call succeeds");
    assert_eq!(found[0].url, "https://r/repo.json");
    assert_eq!(
        gateway.transport.calls.lock().expect("lock")[0].0,
        "take_added_repositories"
    );
}

#[test]
fn load_returns_none_when_runtime_replies_null() {
    let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(Value::Null)));
    assert_eq!(gateway.load("Archive", "u").expect("load succeeds"), None);
}

#[test]
fn remote_failure_is_propagated_unchanged() {
    let failure = GatewayError::Remote {
        kind: "NotImplementedError".into(),
        message: String::new(),
    };
    let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Err(failure.clone())));
    assert_eq!(gateway.search("p", "q", 1), Err(failure));
}

#[test]
fn malformed_reply_becomes_decode_error_naming_the_method() {
    let gateway = RpcProviderGateway::new(ScriptedTransport::replying(Ok(json!("not a page"))));
    match gateway.search("p", "q", 1) {
        Err(GatewayError::Decode { method, .. }) => assert_eq!(method, "search"),
        other => panic!("expected decode error, got {other:?}"),
    }
}

#[test]
fn extract_sends_null_referer_when_absent() {
    let gateway =
        RpcProviderGateway::new(ScriptedTransport::replying(Ok(json!({"success":false}))));
    gateway
        .extract("https://h.test/v", None)
        .expect("extract succeeds");
    let calls = gateway.transport.calls.lock().expect("lock");
    assert_eq!(calls[0].1, json!({"url":"https://h.test/v","referer":null}));
}
