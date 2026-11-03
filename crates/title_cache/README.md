# title_cache

Remembers the stream links found for a title (a movie or one episode) in small JSON files in the
app's data folder, so opening it again does not wait for a full search. It never decides anything
and never fails playback: an unreadable, corrupt or old file is a miss, a failed save is reported
and ignored.

| Part | Responsibility |
|---|---|
| `TitleCache` | `load` / `save` / `update` / `mark_broken` / `forget`, one file per title, atomic writes, one write lock |
| `CachedTitle`, `CachedLink`, `LinkHealth` | the content and the order links are tried in (last good, working, unchecked, broken) |
| `CacheLimits` | fresh under 20 min, stale up to 24 h, 50 titles, 200 links, 1 MB per file |
| `TitleCache::prune` / `size_bytes` / `clear` | cleanup of expired, broken, surplus and leftover files |
| `redact_url` | `host#fingerprint` for logs; stream links carry tokens |

Files: `<folder>/<hash of provider+title>.json`, written aside and renamed.

Test: `cargo test -p title_cache` (real temp folders, a read-only folder, concurrent readers and writers).
