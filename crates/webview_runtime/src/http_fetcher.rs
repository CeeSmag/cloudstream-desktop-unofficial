//! Downloading the setup over HTTPS. Redirects are followed here, one by one, so every address on
//! the way is checked (ureq would follow them without asking). The size is capped while reading,
//! the cancel flag is looked at after every chunk, and the file appears only once complete.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::{
    host_of, is_allowed_setup_address, next_hop, CancelFlag, FetchRequest, SetupError,
    SetupFetcher, MAX_HOPS, MAX_SETUP_BYTES,
};

/// The whole download may take this long (the setup is about 2 MB).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_BYTES: usize = 64 * 1024;

/// Downloads with ureq, following only redirects the address rule allows.
#[derive(Debug, Clone, Copy)]
pub struct UreqSetupFetcher {
    allow: fn(&str) -> bool,
    max_bytes: u64,
}

impl UreqSetupFetcher {
    /// Downloads only from Microsoft over HTTPS, at most [`MAX_SETUP_BYTES`].
    pub fn microsoft() -> Self {
        Self::with_rule(is_allowed_setup_address, MAX_SETUP_BYTES)
    }

    /// Downloads from addresses `allow` accepts, at most `max_bytes` (tests use local servers).
    pub fn with_rule(allow: fn(&str) -> bool, max_bytes: u64) -> Self {
        Self { allow, max_bytes }
    }

    fn check(&self, url: &str) -> Result<(), SetupError> {
        if (self.allow)(url) {
            Ok(())
        } else {
            let host = host_of(url).unwrap_or_else(|| "an unreadable address".into());
            Err(SetupError::WrongHost(host))
        }
    }

    /// The final response after the allowed redirects.
    fn follow(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, SetupError> {
        let agent = agent();
        let mut current = url.to_owned();
        for _ in 0..=MAX_HOPS {
            self.check(&current)?;
            let response = agent
                .get(&current)
                .header(
                    "User-Agent",
                    concat!("CloudStream-Desktop/", env!("CARGO_PKG_VERSION")),
                )
                .call()
                .map_err(|error| SetupError::Download(error.to_string()))?;
            let status = response.status().as_u16();
            if !(300..400).contains(&status) {
                return answered(response, status);
            }
            let location = header(&response, "location")
                .ok_or_else(|| SetupError::Download("a redirect without an address".into()))?;
            current = next_hop(&current, &location)
                .ok_or_else(|| SetupError::Download("a redirect that cannot be followed".into()))?;
        }
        Err(SetupError::Download("too many redirects".into()))
    }
}

impl SetupFetcher for UreqSetupFetcher {
    fn fetch(
        &self,
        request: FetchRequest<'_>,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), SetupError> {
        let response = self.follow(request.url)?;
        let total = header(&response, "content-length")
            .and_then(|text| text.parse::<u64>().ok())
            .unwrap_or(0);
        if total > self.max_bytes {
            return Err(SetupError::TooLarge {
                limit: self.max_bytes,
            });
        }
        let partial = partial_path(request.dest);
        let mut reader = response.into_body().into_reader();
        let copy = Copy {
            limit: self.max_bytes,
            total,
            cancel: request.cancel,
        };
        let copied = copy.run(&mut reader, &partial, progress);
        let result = copied.and_then(|()| {
            std::fs::rename(&partial, request.dest)
                .map_err(|error| SetupError::disk("move", request.dest, &error))
        });
        if result.is_err() {
            let _ = std::fs::remove_file(&partial);
        }
        result
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn answered(
    response: ureq::http::Response<ureq::Body>,
    status: u16,
) -> Result<ureq::http::Response<ureq::Body>, SetupError> {
    if (200..300).contains(&status) {
        Ok(response)
    } else {
        Err(SetupError::Download(format!(
            "the server answered HTTP {status}"
        )))
    }
}

fn partial_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// One capped, cancellable copy from the connection to a file.
struct Copy<'a> {
    limit: u64,
    total: u64,
    cancel: &'a CancelFlag,
}

impl Copy<'_> {
    fn run(
        &self,
        reader: &mut dyn Read,
        dest: &Path,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<(), SetupError> {
        let mut file =
            File::create(dest).map_err(|error| SetupError::disk("create", dest, &error))?;
        let mut buffer = vec![0_u8; CHUNK_BYTES];
        let mut done: u64 = 0;
        loop {
            if self.cancel.is_cancelled() {
                return Err(SetupError::Cancelled);
            }
            let read = reader
                .read(&mut buffer)
                .map_err(|error| SetupError::Download(error.to_string()))?;
            let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
                break;
            };
            done += chunk.len() as u64;
            if done > self.limit {
                return Err(SetupError::TooLarge { limit: self.limit });
            }
            file.write_all(chunk)
                .map_err(|error| SetupError::disk("write", dest, &error))?;
            progress(done, self.total);
        }
        file.flush()
            .map_err(|error| SetupError::disk("write", dest, &error))
    }
}

#[cfg(test)]
#[path = "http_fetcher_tests.rs"]
mod tests;
