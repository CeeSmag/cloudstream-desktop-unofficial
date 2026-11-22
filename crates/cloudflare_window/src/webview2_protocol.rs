//! Correlates WebView2's top-level document and intercepts its response before it can paint.

#[path = "webview2_response.rs"]
mod response;

use serde_json::{json, Value};

use super::*;
use crate::verification::{matches_target, MainResponse, ObservedResponse};
use response::PausedResponse;

pub(super) struct Observer {
    pub(super) source: BrowserSource,
    host: Weak<WebViewHost>,
    pub(super) committed: Cell<Option<DocumentId>>,
    document: Cell<Option<DocumentId>>,
    frame: RefCell<String>,
    loader: RefCell<Option<MainLoader>>,
}

struct MainLoader {
    request: String,
    loader: String,
    document: DocumentId,
}

impl Observer {
    pub(super) fn new(source: BrowserSource, host: Weak<WebViewHost>) -> Self {
        Self {
            source,
            host,
            committed: Cell::new(None),
            document: Cell::new(None),
            frame: RefCell::default(),
            loader: RefCell::default(),
        }
    }

    pub(super) fn initialize(self: &Rc<Self>) {
        let observer = Rc::clone(self);
        self.call("Page.enable", "{}", move |_| observer.identify_frame());
    }

    fn identify_frame(self: &Rc<Self>) {
        let observer = Rc::clone(self);
        self.call("Page.getFrameTree", "{}", move |raw| {
            let frame = serde_json::from_str::<Value>(&raw).ok().and_then(|tree| {
                tree.pointer("/frameTree/frame/id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
            let Some(frame) = frame else {
                observer.source.unavailable(
                    "interactive verification is unavailable",
                    "no top-level frame",
                );
                return;
            };
            observer.frame.replace(frame);
            observer.enable_responses();
        });
    }

    fn enable_responses(self: &Rc<Self>) {
        let observer = Rc::clone(self);
        self.call("Network.enable", "{}", move |_| {
            let source = observer.source.clone();
            observer.call(
                "Fetch.enable",
                r#"{"patterns":[{"resourceType":"Document","requestStage":"Response"}]}"#,
                move |_| {
                    source.send(BrowserEvent::Ready);
                },
            );
        });
    }

    pub(super) fn started(&self, document: DocumentId) {
        let Some(host) = self.host.upgrade() else {
            return;
        };
        self.committed.set(None);
        if let Err(error) = host.controller().set_visible(false) {
            self.source
                .unavailable("could not hide the verification document", error);
            return;
        }
        if self.document.replace(Some(document)) != Some(document) {
            self.loader.replace(None);
            self.source.send(BrowserEvent::Started(document));
        }
    }

    fn request(&self, event: &Value) {
        if event.get("type").and_then(Value::as_str) != Some("Document")
            || event.get("frameId").and_then(Value::as_str) != Some(&*self.frame.borrow())
        {
            return;
        }
        let (Some(request), Some(loader), Some(document)) = (
            event.get("requestId").and_then(Value::as_str),
            event.get("loaderId").and_then(Value::as_str),
            self.document.get(),
        ) else {
            return;
        };
        self.loader.replace(Some(MainLoader {
            request: request.to_owned(),
            loader: loader.to_owned(),
            document,
        }));
    }

    fn committed(&self, event: &Value) {
        if event.pointer("/frame/id").and_then(Value::as_str) != Some(&*self.frame.borrow()) {
            return;
        }
        let loader = self.loader.borrow();
        let Some(loader) = loader.as_ref().filter(|loader| {
            event.pointer("/frame/loaderId").and_then(Value::as_str) == Some(loader.loader.as_str())
                && self.document.get() == Some(loader.document)
        }) else {
            return;
        };
        if let Some(url) = event
            .pointer("/frame/url")
            .and_then(Value::as_str)
            .and_then(|url| Url::parse(url).ok())
        {
            self.committed.set(Some(loader.document));
            self.source
                .send(BrowserEvent::Committed(loader.document, url));
        }
    }

    fn paused(self: &Rc<Self>, paused: PausedResponse) {
        if paused.frame_id != *self.frame.borrow() {
            self.resume(&paused.request_id, false);
            return;
        }
        let Some((document, response)) = self.read_paused(&paused) else {
            self.resume(&paused.request_id, true);
            return;
        };
        if response.challenge || !matches_target(&self.source.target, &response.url) {
            self.release(
                &paused.request_id,
                ObservedResponse {
                    document,
                    response,
                    cookies: Vec::new(),
                },
            );
        } else {
            self.observe_cookies(document, paused.request_id, response);
        }
    }

    fn read_paused(&self, paused: &PausedResponse) -> Option<(DocumentId, MainResponse)> {
        if paused.network_id.is_none() {
            self.source.unavailable(
                "interactive verification is unavailable",
                "the main response has no navigation identifier",
            );
            return None;
        }
        // The loader may already have been replaced by a newer document.
        let document = self
            .loader
            .borrow()
            .as_ref()
            .filter(|loader| Some(loader.request.as_str()) == paused.network_id.as_deref())
            .map(|loader| loader.document)?;
        let response = paused.response().or_else(|| {
            self.source.unavailable(
                "could not read the verification response",
                "the main document had no HTTP response",
            );
            None
        })?;
        Some((document, response))
    }

    fn observe_cookies(
        self: &Rc<Self>,
        document: DocumentId,
        request: String,
        response: MainResponse,
    ) {
        let Some(host) = self.host.upgrade() else {
            return;
        };
        let observer = Rc::clone(self);
        let on_error = request.clone();
        if let Err(error) = read_cookies(&host, &self.source.target, move |result| {
            let cookies = result.unwrap_or_else(|error| {
                tracing::warn!(%error, "WebView2 could not read scoped verification cookies");
                Vec::new()
            });
            observer.release(
                &request,
                ObservedResponse {
                    document,
                    response,
                    cookies,
                },
            );
        }) {
            self.resume(&on_error, true);
            self.source
                .unavailable("could not inspect verification cookies", error);
        }
    }

    fn release(&self, request: &str, observed: ObservedResponse) {
        let refuse =
            observed.response.attachment || observed.response.leaves_origin(&self.source.target);
        if self.document.get() == Some(observed.document) {
            self.source.send(BrowserEvent::Response(observed));
        }
        self.resume(request, refuse);
    }

    fn resume(&self, request: &str, refuse: bool) {
        if refuse {
            self.call(
                "Fetch.failRequest",
                &json!({"requestId": request, "errorReason": "Aborted"}).to_string(),
                |_| {},
            );
        } else {
            self.call(
                "Fetch.continueRequest",
                &json!({"requestId": request}).to_string(),
                |_| {},
            );
        }
    }

    fn call(&self, method: &'static str, params: &str, done: impl FnOnce(String) + 'static) {
        let Some(host) = self.host.upgrade() else {
            return;
        };
        let source = self.source.clone();
        let sent = host
            .webview()
            .call_dev_tools_protocol_method(method, params, move |result| match result {
                Ok(raw) => done(raw),
                Err(error) => source.unavailable(method, error),
            });
        if let Err(error) = sent {
            self.source.unavailable(method, error);
        }
    }
}

pub(super) fn subscribe(
    native: &WebViewHost,
    observer: &Rc<Observer>,
) -> windows_webview::Result<()> {
    let request = Rc::clone(observer);
    let commit = Rc::clone(observer);
    let paused = Rc::clone(observer);
    native.retain_all([
        native
            .webview()
            .on_dev_tools_protocol_event("Network.requestWillBeSent", move |args| {
                if let Ok(event) = serde_json::from_str(&args.parameter_object_as_json()) {
                    request.request(&event);
                }
            })?,
        native
            .webview()
            .on_dev_tools_protocol_event("Page.frameNavigated", move |args| {
                if let Ok(event) = serde_json::from_str(&args.parameter_object_as_json()) {
                    commit.committed(&event);
                }
            })?,
        native
            .webview()
            .on_dev_tools_protocol_event("Fetch.requestPaused", move |args| {
                match serde_json::from_str(&args.parameter_object_as_json()) {
                    Ok(event) => paused.paused(event),
                    Err(error) => paused
                        .source
                        .unavailable("could not classify the verification document", error),
                }
            })?,
    ]);
    Ok(())
}
