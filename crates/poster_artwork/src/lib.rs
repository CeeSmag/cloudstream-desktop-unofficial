//! Poster artwork for the UI: address clean-up, download, shrink, memory and disk caches.
#![forbid(unsafe_code)]

mod artwork_address;
mod artwork_decode;
mod artwork_disk_cache;
mod artwork_error;
mod artwork_fetcher;
mod artwork_load;
mod artwork_loader;
#[cfg(test)]
mod artwork_loader_tests;
mod artwork_memory_cache;

pub use artwork_address::poster_address;
pub use artwork_decode::{
    decode_artwork, decode_artwork_to, ArtworkSize, DecodedArtwork, MAX_ARTWORK_WIDTH,
    TILE_ARTWORK_WIDTH,
};
pub use artwork_disk_cache::DiskCache;
pub use artwork_error::ArtworkError;
pub use artwork_fetcher::{ArtworkFetcher, ArtworkHeaders, UreqArtworkFetcher, MAX_IMAGE_BYTES};
pub use artwork_loader::{
    ArtworkCallback, ArtworkLoader, ArtworkLoaderConfig, ArtworkRequest, ArtworkResult,
    DEFAULT_MEMORY_BUDGET_BYTES, DEFAULT_WORKERS,
};
