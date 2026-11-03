# Cache plan

Status: **decisions made, building.** Steps 1 (the `title_cache` crate), 2 (hit / stale / miss in the opening flow, save, logs, loading-screen text) and 3 (the fallback controller, `source_fallback.rs`) are done; removing a title from Continue Watching also forgets its saved sources and subtitle files; see "Order of work" for the rest.

Where the project stands (branch `feat/fast-sources`):
- `36235c9` the provider's subtitle files are downloaded, cleaned and kept on disk (`crates/subtitle_store`); the preferred language turns on once per stream.
- `d0342eb` opening a video waits for the whole search and all checks, then plays the best working source. Skip (shown only once a source works) plays at once and leaves everything running. A provider that gives no stream in 30 s is an error; the whole wait is capped at 60 s (`WaitLimits` in `desktop_tauri/src/check_progress.rs`).
- Still not merged or pushed: `feat/extensions-page`, `feat/search-all-extensions`, `feat/fast-sources` into main.

## Principles
- **Never fail playback because of the cache.** Any cache problem (unreadable file, full disk, wrong version) means "treat as a miss", log a warning, and carry on as if there were no cache.
- **A cache hit never replaces the truth.** It starts playback faster. A fresh search still runs in the background and refreshes the cache.
- **Hits are the exception to the wait-for-everything rule.** On a hit the app does not wait for the whole search. On a miss it still waits for everything, with the 30 s and 60 s limits, and skip works as before.
- **Secrets stay out of logs.** Stream links often carry tokens, so logs show only host, a short hash and counts. The existing `playing stream` log line in `video_opening.rs` prints the full URL and gets scrubbed too.

## 1. Where it lives
A new crate `title_cache`, with a README (≤ 40 lines) and tests. It depends on `subtitle_store` for the shared key function (`cache_key`, now private there) and for deleting a title's subtitle list. Files go in the app data folder next to `subtitles/`, not Program Files (which can be read-only).

```
<data>/sources/<hash of provider+title>.json
```

The title key is the one already used for subtitles: `"{provider}\n{media_key}"`.

One file per title or episode, containing:
- **Header:** format `version`, when it was saved, whether the search had finished (`complete`).
- **Links:** each `PlayableLink` (source, name, url, referer, quality, headers, type).
- **Last check:** the result for each link (working, broken or unchecked) and when it was checked.
- **Last played:** the last link that played well.

Writes go to a temporary file and are then renamed, so a crash never leaves a half-written file.

## 2. Rules (the defaults)
| Rule | Default |
|---|---|
| Fresh (play at once) | saved under 20 min ago |
| Stale (test first) | 20 min to 24 h |
| Older than that | deleted |
| Titles kept | newest 50 (files are a few KB; LRU eviction) |
| Links kept per title | best 200 |
| File size limit | 1 MB (larger is refused) |
| Finished | position at 95% or more, or the player reached the end |
| Clock set backwards | treated as stale |

Why 20 minutes: the reference app keeps links for 20 minutes because many stream links expire.

## 3. Flows
- **Fresh hit:** the cached links fill the Sources list, the last good one (else the best working) starts at once, and the saved position is used. Meanwhile a fresh search runs and the top links are re-checked. The cache is rewritten when the search ends.
- **Stale hit:** the best 3 cached links are tested in parallel, with a 3 s cap. The first one that works starts. If none works, the normal full flow runs, and the loading screen says "Saved sources are out of date, searching again".
- **Miss:** the normal flow, then save when the search finishes. If you leave or skip before it finishes and at least one source worked, a partial list is saved and marked as partial, so it is refreshed next time.
- **Provider down but cache present:** the title still plays from the cache.
- **Saving:** at most once per search end or close, never per check, so there is no write storm.

## 4. Fallback when a cached or any link fails
1. **Next source:** the app tries the next best known-working source, then the best unchecked cached one, then a fresh search. It resumes at the same position.
2. **Limits:** at most 3 attempts and 20 s in total. Each source is tried once.
3. **Final error:** if all fail, one message: "3 sources failed (reason, reason, reason)".
4. **Remembered failures:** failed links are marked broken in the cache so the next open skips them.
5. **Cancelling:** a manual source switch or closing the video cancels the fallback. A generation number on each attempt stops a late attempt from touching a newer video.

## 5. Finished videos and cleanup
- **Finished:** when you exit at 95% or more, or the video ends, that title's source file and subtitle list are removed. Watch progress stays.
- **Unfinished:** the cache stays until it ages out.
- **Startup and after writes:** a cleanup removes expired entries, leftover `.tmp` files from crashes, and entries beyond the limits.
- **Settings:** shows the cache size and a "Clear cache" button, and a General option "Delete all app data" (confirmation first) that removes the whole data folder's contents.

## 6. Subtitles (separate step)
- **Download up front:** only your preferred languages, at most 10 per video and 3 per language.
- **Other languages:** not downloaded and not listed. (Decided: no "download on click".)
- **No preferred languages set:** the first 3 files, in the provider's order, are downloaded.
- Still only the installed provider's subtitles, never another source.

## 7. In-session buffer (optional, last)
mpv keeps a window around your position, about 10 minutes back and ahead, in a temporary file under `<data>/playback-cache/` (`cache-on-disk`, `demuxer-cache-dir`, `demuxer-readahead-secs`, `demuxer-max-bytes`, `demuxer-max-back-bytes`). It is capped in size, deleted on exit, and orphans from crashes are swept at startup. The window is set by bytes, so it covers fewer minutes for 4K than for 1080p (20 min ≈ 0.7–1.2 GB at 1080p, 2–4 GB at 4K).

Not planned: caching the video bytes across exits. It needs a local caching proxy for HLS, saves little (the reopen delay is finding sources, not buffering), and turns the app into a downloader.

## 8. Logs
- **Cache lookup:** hit, stale or miss, with age and link count.
- **Save:** link count, bytes, and whether partial.
- **Re-check:** how many links worked.
- **Fallback:** each attempt number, reason and elapsed time.
- **Cleanup:** forget on finish, with the reason. Prune with counts, and the size after.
- **Failures:** every cache failure, with the reason.

Everything at `debug`, failures at `warn`. Never a full URL.

## 9. Error handling
| Problem | Behavior |
|---|---|
| File corrupt, empty or truncated | miss, file deleted, warning |
| Unknown format version | miss, file deleted |
| Disk full or no permission | warn once, play without cache |
| File over 1 MB | refused |
| Two writers at once | per-title lock plus atomic rename |
| Cached link dead | fallback (section 4) |
| Cache folder missing | recreated; if that fails, no cache |

## 10. Tests (per RULES.md: T1–T4, race tests, mutation checks)
- **Unit:**
  - Freshness at the boundaries (19:59, 20:00, 24 h, negative age).
  - Order with the last good link first.
  - Round-trip save and load, plus an unknown extra field.
  - The caps and LRU eviction.
  - Redacting URLs in logs.
- **Real-disk (temp folders):**
  - A leftover temporary file is ignored.
  - Corrupt, truncated, empty and wrong-version files.
  - A read-only folder.
  - Cleanup by age, count and size.
  - Forgetting a title removes its source file and subtitle list.
- **Opening flow (fake provider, existing rig in `opening_flow_tests.rs`):**
  - A fresh hit plays without waiting for the search, and the cache is refreshed.
  - A stale hit with a dead top link moves to the next one.
  - A stale hit with every link dead runs the normal flow.
  - A miss saves at the end.
  - A partial save on close.
  - The provider down with a cache present still plays.
  - A finished video is forgotten and an unfinished one is kept.
- **Fallback:**
  - The player fails, the next source plays at the same position.
  - After 3 failures the error lists all three.
  - A manual switch cancels the fallback.
  - Close cancels the fallback.
- **Race:**
  - Fallback against close and switch.
  - Concurrent saves from search end and close.
  - 50 open and close cycles leave no temporary files or threads.
  - Concurrent readers and writers of one file.
- **Real libmpv (ignored tests, `CS_LIBMPV_PATH=C:\cs3d\mpv\libmpv-2.dll`):** a failing stream falls back to a second one at the same position; the temporary buffer folder is created and removed.
- **Frontend (vitest):** the "saved sources" and "out of date" messages, cache size text, and the delete-all-data confirmation.
- **Mutation checks** for each rule above: revert the fix and the test must fail.

## 11. Order of work (one commit each; gates: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `tsc` + vitest, files ≤ 300 lines, functions ≤ 40 lines)
1. The `title_cache` crate: store, rules, cleanup and tests.
2. Wire the opening flow: hit, stale, miss, save, background re-check and refresh, logs. Relaunch the app for the user to test.
3. The fallback controller, with its tests. Relaunch the app for the user to test.
4. Finished-video cleanup, cleanup at startup, and the Settings size and clear button.
5. The subtitle policy and on-demand entries in the menu.
6. The optional mpv buffer window.

## Decisions (answered)
1. Defaults of section 2 are accepted (20 min fresh, 24 h stale, 50 titles, 95% finished).
2. Subtitles that are not preferred are **not downloaded** at all (no "download on click" entries).
3. Step 6 (the mpv buffer window) is wanted.
4. The cache lives in the app data folder. Settings, General also gets an option to delete the whole
   app data folder (all caches, subtitles and saved data), with a confirmation.
