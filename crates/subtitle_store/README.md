# subtitle_store

Downloads the subtitle files the installed provider offers, cleans them and keeps them on disk, so
the player only ever opens local files. No subtitles come from anywhere but the provider.

| Part | Responsibility |
|---|---|
| `SubtitleFetcher` / `UreqSubtitleFetcher` | one HTTP download with timeouts, a size cap and the provider's headers |
| `normalize_subtitle` | unzip or gunzip, decode (BOM, UTF-16, Windows-1252), refuse web pages, detect the format |
| `hls_subtitles` | flatten a subtitle playlist into one WebVTT text |
| `SubtitleCache` | files named by a hash of their address, a time to live, a saved list per title |
| `SubtitleCache::keep_only_title` | only the video opened last keeps subtitles: the other titles' lists and every file its own list does not name are removed, and a late download of a replaced video is not noted (same lock) and its file removed; `discard_unclaimed` removes a file that finished after its video was replaced and that no video took |
| `SubtitleLoader` | cache lookup, then download with retries, then clean and store |
| `SubtitlePrefetch` | worker threads that download many files at once, preferred languages first |

Test: `cargo test -p subtitle_store` (includes a real local HTTP server and race tests).
