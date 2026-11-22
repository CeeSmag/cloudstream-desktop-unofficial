//! Main-document response fields from WebView2's DevTools protocol.

use serde::Deserialize;
use url::Url;

use crate::verification::MainResponse;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PausedResponse {
    pub(super) request_id: String,
    pub(super) network_id: Option<String>,
    pub(super) frame_id: String,
    request: RequestUrl,
    response_status_code: Option<u16>,
    response_headers: Option<Vec<Header>>,
}

#[derive(Deserialize)]
struct RequestUrl {
    url: String,
}

#[derive(Deserialize)]
struct Header {
    name: String,
    value: String,
}

impl PausedResponse {
    pub(super) fn response(&self) -> Option<MainResponse> {
        let headers = self.response_headers.as_deref()?;
        Some(MainResponse::from_headers(
            Url::parse(&self.request.url).ok()?,
            self.response_status_code?,
            |key| {
                headers
                    .iter()
                    .find(|header| header.name.eq_ignore_ascii_case(key))
                    .map(|header| header.value.clone())
            },
        ))
    }
}
