# subtitle_store

Downloads the subtitle files a provider offers, cleans them up (unpacks, decodes, detects the
kind) and keeps them on disk, so the player only ever opens local files. No subtitles come from
anywhere but the provider.

| Part | Responsibility |
|---|---|
| `SubtitleFetcher` / `UreqSubtitleFetcher` | one HTTP download: 10 s timeout, 8 MB cap, the provider's headers |
| `normalize_subtitle` | unzip or gunzip, decode (BOM, UTF-16, Windows-1252), refuse web pages, detect the format |
| `hls_subtitles` | flatten a segmented M3U8 subtitle playlist into one WebVTT file |
| `SubtitleCache` | files under `files/` named by a hash of their address plus extension, a time to live, a JSONL list per title under `titles/`; `prune` |
| `SubtitleCache::keep_only_title` | only the video opened last keeps subtitles: the other titles' lists and every file its own list does not name are removed, and a late download of a replaced video is not noted (same lock) and its file removed; `discard_unclaimed` removes a file that finished after its video was replaced and that no video took |
| `SubtitleLoader` | cache lookup, then download with retries (`LoadOptions`: 3 attempts, 400 ms backoff), then clean and store |
| `SubtitlePrefetch` | worker threads that download many files at once, preferred languages first (`LanguageRank`); ranks of `SKIP_DOWNLOAD_RANK` are not queued; stoppable via `KeepGoing` |

Files: `<folder>/files/<hash>.<ext>`, lists in `<folder>/titles/<hash>.jsonl` (TTL set by the caller).

Test: `cargo test -p subtitle_store` (includes a real local HTTP server and race tests).
