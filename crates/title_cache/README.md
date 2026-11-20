# title_cache

Remembers the stream links found for a title (a movie or one episode) in small JSON files in the
app's data folder, so opening it again does not wait for a full search. It never decides anything
and never fails playback: an unreadable, corrupt or old file is a miss, a failed save is reported
and ignored.

| Part | Responsibility |
|---|---|
| `TitleCache` | `load` / `save` / `forget`, one file per title, atomic writes, one write lock |
| `TitleCache::invalidate_now` / `set_valid_from` | after a provider switch, saves from before that time are a miss (`Miss::Invalidated`) and go on the next `prune` |
| `CachedTitle`, `CachedLink`, `LinkHealth`, `Freshness` | the content and the order links are tried in (last good, working, unchecked, broken); fresh vs stale on load |
| `TitleCache::keep_only` / `save_kept`, `KeptTitle`, `KeptSave` | only the video opened last keeps saved sources: opening one removes every other title's file, and a late save of an opening that was replaced is refused (same lock) |
| `CacheLimits` | fresh under 20 min, stale up to 24 h, 50 titles, 200 links, 1 MB per file |
| `TitleCache::prune`, `PruneReport` | cleanup of expired, invalidated, corrupt, surplus and leftover files |

Files: `<folder>/<hash of title key>.json`, written aside and renamed.

Test: `cargo test -p title_cache` (real temp folders, a read-only folder, concurrent readers and writers).
