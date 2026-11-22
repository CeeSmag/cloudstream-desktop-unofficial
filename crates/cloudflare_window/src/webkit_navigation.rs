//! Tracks the WebKit main resource from provisional load through commit.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk::prelude::WidgetExt;
use url::Url;
use webkit2gtk::glib::{prelude::*, WeakRef};
use webkit2gtk::{
    CookieManager, LoadEvent, PolicyDecision, PolicyDecisionExt, PolicyDecisionType,
    ResponsePolicyDecision, ResponsePolicyDecisionExt, URIResponse, URIResponseExt, WebResource,
    WebView, WebViewExt,
};

use super::cookies::read_cookies;
use super::BrowserSource;
use crate::verification::{
    matches_target, BrowserEvent, Cookies, DocumentId, MainResponse, ObservedResponse,
};

pub(super) struct Observer {
    pub(super) source: BrowserSource,
    pub(super) cookies: CookieManager,
    pub(super) committed: Cell<Option<DocumentId>>,
    current: Cell<u64>,
    resource: RefCell<WeakRef<WebResource>>,
    responses: RefCell<ResponseOrder>,
}

#[derive(Default)]
struct ResponseOrder {
    issued: u64,
    delivered: u64,
    ready: BTreeMap<u64, ReadyResponse>,
}

struct ReadyResponse {
    observed: ObservedResponse,
    policy: Option<ResponsePolicy>,
}

struct ResponsePolicy {
    decision: PolicyDecision,
    allowed: bool,
}

impl Observer {
    pub(super) fn install(
        view: &WebView,
        source: BrowserSource,
        cookies: CookieManager,
    ) -> Rc<Self> {
        let observer = Rc::new(Self {
            source,
            cookies,
            current: Cell::new(0),
            committed: Cell::new(None),
            resource: RefCell::default(),
            responses: RefCell::default(),
        });
        observe_load(view, &observer);
        observe_redirects(view, &observer);
        control_response(view, &observer);
        observer
    }

    fn current(&self) -> DocumentId {
        DocumentId(self.current.get())
    }

    fn matches_resource(&self, view: &WebView) -> bool {
        self.resource
            .borrow()
            .upgrade()
            .is_some_and(|resource| view.main_resource().as_ref() == Some(&resource))
    }

    fn observe(
        self: &Rc<Self>,
        document: DocumentId,
        response: MainResponse,
        policy: Option<ResponsePolicy>,
    ) {
        let sequence = {
            let mut order = self.responses.borrow_mut();
            order.issued += 1;
            order.issued
        };
        if response.challenge || !matches_target(&self.source.target, &response.url) {
            self.deliver(
                sequence,
                ReadyResponse::new(document, response, Vec::new(), policy),
            );
            return;
        }
        let observer = Rc::clone(self);
        read_cookies(&self.cookies, &self.source.target, move |result| {
            let cookies = result.unwrap_or_else(|error| {
                tracing::warn!(%error, "WebKit could not read scoped verification cookies");
                Vec::new()
            });
            observer.deliver(
                sequence,
                ReadyResponse::new(document, response, cookies, policy),
            );
        });
    }

    fn deliver(&self, sequence: u64, response: ReadyResponse) {
        self.responses.borrow_mut().ready.insert(sequence, response);
        while let Some(ready) = self.take_next_response() {
            let current = ready.observed.document == self.current();
            if current {
                self.source.send(BrowserEvent::Response(ready.observed));
            }
            if let Some(policy) = ready.policy {
                // A document can run a redirect as soon as the policy allows it.
                // Record the original response and cookies before that happens.
                if current && policy.allowed {
                    policy.decision.use_();
                } else {
                    policy.decision.ignore();
                }
            }
        }
    }

    fn take_next_response(&self) -> Option<ReadyResponse> {
        let mut order = self.responses.borrow_mut();
        let next = order.delivered + 1;
        let ready = order.ready.remove(&next)?;
        order.delivered = next;
        Some(ready)
    }
}

impl ReadyResponse {
    fn new(
        document: DocumentId,
        response: MainResponse,
        cookies: Cookies,
        policy: Option<ResponsePolicy>,
    ) -> Self {
        Self {
            observed: ObservedResponse {
                document,
                response,
                cookies,
            },
            policy,
        }
    }
}

fn observe_load(view: &WebView, observer: &Rc<Observer>) {
    let observer = Rc::clone(observer);
    view.connect_load_changed(move |view, event| match event {
        LoadEvent::Started => {
            let document = DocumentId(observer.current.get() + 1);
            observer.current.set(document.0);
            observer.resource.replace(WeakRef::new());
            observer.committed.set(None);
            view.set_visible(false);
            observer.source.send(BrowserEvent::Started(document));
        }
        LoadEvent::Committed if observer.matches_resource(view) => {
            let document = observer.current();
            if let Some(url) = view.uri().and_then(|uri| Url::parse(&uri).ok()) {
                observer.committed.set(Some(document));
                observer.source.send(BrowserEvent::Committed(document, url));
            }
        }
        _ => {}
    });
}

fn observe_redirects(view: &WebView, observer: &Rc<Observer>) {
    let observer = Rc::clone(observer);
    view.connect_resource_load_started(move |view, resource, _| {
        if view.main_resource().as_ref() != Some(resource) {
            return;
        }
        observer.resource.replace(resource.downgrade());
        let document = observer.current();
        let redirects = Rc::clone(&observer);
        // WebKit supplies null for the initial request. The typed wrapper
        // for this signal does not represent the nullable response.
        resource.connect_local("sent-request", false, move |arguments| {
            if document == redirects.current() {
                let redirect = arguments.get(2)?.get::<Option<URIResponse>>().ok()??;
                if let Some(response) = read_response(&redirect) {
                    redirects.observe(document, response, None);
                }
            }
            None
        });
    });
}

fn control_response(view: &WebView, observer: &Rc<Observer>) {
    let observer = Rc::clone(observer);
    view.connect_decide_policy(move |view, decision, kind| {
        if kind != PolicyDecisionType::Response {
            return false;
        }
        let Some(policy) = decision
            .downcast_ref::<ResponsePolicyDecision>()
            .filter(|policy| policy.is_main_frame_main_resource())
        else {
            return false;
        };
        apply_main_policy(&observer, view, policy);
        true
    });
}

fn apply_main_policy(observer: &Rc<Observer>, view: &WebView, policy: &ResponsePolicyDecision) {
    view.set_visible(false);
    if !observer.matches_resource(view) {
        policy.ignore();
        observer.source.unavailable(
            "interactive verification is unavailable",
            "WebKit did not identify the main resource",
        );
        return;
    }
    let Some(response) = policy
        .response()
        .and_then(|response| read_response(&response))
    else {
        policy.ignore();
        observer.source.unavailable(
            "could not read the verification response",
            "the main document had no HTTP response",
        );
        return;
    };
    let allowed = !response.attachment
        && policy.is_mime_type_supported()
        && !response.leaves_origin(&observer.source.target);
    observer.observe(
        observer.current(),
        response,
        Some(ResponsePolicy {
            decision: policy.clone().upcast(),
            allowed,
        }),
    );
}

fn read_response(response: &URIResponse) -> Option<MainResponse> {
    let url = Url::parse(&response.uri()?).ok()?;
    let headers = response.http_headers();
    let status = u16::try_from(response.status_code()).ok()?;
    Some(MainResponse::from_headers(url, status, |name| {
        headers
            .as_ref()
            .and_then(|headers| headers.one(name))
            .map(|header| header.to_string())
    }))
}
