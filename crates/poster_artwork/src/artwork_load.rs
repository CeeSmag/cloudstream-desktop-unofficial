//! One download or disk hit for a queued poster job.

use std::sync::Arc;

use crate::artwork_loader::{ArtworkResult, Job, Shared};
use crate::{decode_artwork_to, DecodedArtwork};

pub(crate) fn load(shared: &Shared, job: &Job) -> ArtworkResult {
    if let Some(artwork) = from_disk(shared, job) {
        return Ok(artwork);
    }
    from_network(shared, job)
}

fn from_disk(shared: &Shared, job: &Job) -> Option<Arc<DecodedArtwork>> {
    if !job.persist {
        return None;
    }
    let bytes = shared.disk.as_ref().and_then(|disk| disk.load(&job.url))?;
    match decode_artwork_to(&bytes, job.size.max_width()) {
        Ok(artwork) => Some(Arc::new(artwork)),
        Err(error) => {
            tracing::debug!(%error, "a saved poster was unreadable; downloading it again");
            None
        }
    }
}

fn from_network(shared: &Shared, job: &Job) -> ArtworkResult {
    match shared.fetcher.fetch(&job.url, &job.headers) {
        Ok(bytes) => decode_fetched(shared, job, bytes),
        Err(error) => fail(job, error),
    }
}

fn decode_fetched(shared: &Shared, job: &Job, bytes: Vec<u8>) -> ArtworkResult {
    match decode_artwork_to(&bytes, job.size.max_width()) {
        Ok(artwork) => {
            if job.persist {
                if let Some(disk) = &shared.disk {
                    disk.save(&job.url, &bytes);
                }
            }
            Ok(Arc::new(artwork))
        }
        Err(error) => fail(job, error),
    }
}

fn fail(job: &Job, error: crate::ArtworkError) -> ArtworkResult {
    let host = core_model::redact_url(&job.url);
    tracing::warn!(%host, %error, "a poster could not be loaded; the tile shows a placeholder");
    Err(error)
}
