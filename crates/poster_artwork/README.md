# poster_artwork

Poster and artwork images for the UI, loaded off the UI thread. The UI asks for an address and
gets decoded RGBA pixels back once; it never touches the network itself.

| Part | Responsibility |
|---|---|
| `poster_address` | cleans a provider's address: `//host` → `https://host`, spaces and non-ASCII percent-encoded, TMDB `/t/p/original/` (and widths over 500) → `/t/p/w342/` |
| `ArtworkFetcher` / `UreqArtworkFetcher` | one download: 10 s timeout, browser User-Agent, caller `ArtworkHeaders` (provider poster headers or site `Referer`), 12 MB cap |
| `decode_artwork` | JPEG, PNG, WebP, GIF → RGBA, shrunk to at most 360 px wide |
| `DiskCache` | downloaded files in `<data>/artwork/` named by address hash (`.img`), 14 days, at most 256 MB (`prune`) |
| `ArtworkLoader` | 6 worker threads, newest request first, one download per address however many tiles ask, 144 MB LRU memory, one answer per request (also on shutdown); `request` uses disk, `request_ephemeral` skips it (details poster), `cached` / `forget_memory` for the memory cache |

Why the TMDB rewrite: TMDB-based providers hand out `original` images, often several megabytes.
The old UI cut downloads at 2 MB, so those posters never decoded and Home showed no pictures.

Built in `app_shell::service_wiring::build_artwork`, handed to the UI as `UiServices::artwork`.

Test: `cargo test -p poster_artwork` (T1 rules, race tests of the loader with a scripted fetcher,
T2 downloads from a local web server).
