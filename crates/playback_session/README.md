# playback_session

Shell-agnostic stream opening and live source refresh. Searches provider links via `StreamResolver`
on a dedicated poller while checking candidates concurrently in rank order with `StreamChecker`,
seeds from `TitleCache` (fresh titles start at once; stale titles get a short probe window),
supports subtitle-priority gating of background checks (`CheckPriority`), and enforces the open gate
(full search + all checks, viewer skip, or 60 s total cap; 30 s empty fail), picking the **best**
working stream by `link_rank`.

| Part | Responsibility |
|---|---|
| `run_open` / `OpenRequest` | Full stream open with progress, incremental snapshot, and first-working hooks |
| `refresh_sources` / `RefreshRequest` | Drops cached links and re-searches while keeping the playing stream pinned |
| `open_with` / `check_and_pick` / `OpenTarget` | Core open loop over a `StreamResolver` and optional `OpenCache` |
| `OpenControl` / `LiveSources` / `CheckPriority` / `LoadFocus` | Shared skip/cancel/subtitle-priority/focus controls and live snapshot of streams/subtitles |
| `QueueBudget` / `subtitle_rank` / `matches_subtitle_label` | Failure-refunded external subtitle budget, reserve promotion, and preferred-language satisfaction |
| `save_open_kept` / `cached_title_from_open` / `title_key` | Converts `CheckedOpen` into `CachedTitle` and saves under `TitleCache::keep_only` |
| `OpenProgress` / `CheckedOpen` / `OpenError` | Progress counters, verified link + subtitle list, and structured failure reasons |

Test: `cargo test -p playback_session` (includes scripted provider opening flows and race tests).
