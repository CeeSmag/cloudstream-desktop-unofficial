# Episode flow plan: progress marks, next/previous, auto-next, skip intro

Status: planned 2026-11-04. Written after reading the reference app (`source code/app/.../utils/videoskip/*`,
`ui/player/GeneratorPlayer.kt`, `CS3IPlayer.kt`, `PlayerView.kt`).

## What the reference app does

| Feature | Reference behaviour |
|---|---|
| Skip intro / outro / recap | **Not from the provider.** `SkipAPI.videoStamps` asks web databases, in order: AniSkip (anime, by MAL id), TheIntroDB (by TMDb or IMDb id), IntroDB (by IMDb id), AnimeSkip. The first non-empty answer wins and is cached per episode. Each stamp is `{type, startMs, endMs}`. |
| "Next episode" prompt at the credits | A stamp that ends within 20 s of the episode's end (`episodeDurationMs - endMs < 20_000`) becomes a **Next Episode** button instead of a skip button. |
| Auto next when the video ends | `STATE_ENDED` → `NextEpisode` event, only if the "autoplay next" setting is on (default on). |
| Next button without stamps | Shown whenever a next episode exists (and the opening button is not showing). |
| Finished mark | `NEXT_WATCH_EPISODE_PERCENTAGE = 90` moves the resume record to the next episode. |

## What our providers give us

The sidecar already returns `syncData` (for CineStream: `{"": "{\"Imdb\":\"tt18546730\"}"}`) and `uniqueUrl`
with every `load` reply (see `crates/extension_runtime/tests/fixtures/sidecar_replies/load_series.json`).
`core_model::LoadDetails` drops them today. Providers do **not** carry skip times themselves, so the ids are
the only thing needed; the times come from TheIntroDB / IntroDB / AniSkip, which answer without an API key
(checked 2026-11-04 for `tt18546730` S1E1).

## Decisions

1. Finished mark at **80 %** (user's rule), shown in the episode list only. The 90 % rule for moving the
   Continue Watching pointer stays as it is.
2. Auto next only when the video really ended near its end (position >= 90 %), so a cut-off stream does not
   skip the episode. Setting "Play the next episode automatically", default on.
3. Skip times are looked up online by id (IMDb/TMDb/MAL). Setting "Look up skip times online", default on,
   because it sends the title id to a third-party site.
4. Order of episodes for next/previous is season then episode number, not the provider's list order.

## Steps (each ends with all gates green and a commit)

1. **Episode marks on the title page.** `LibraryService::show_progress`, command `get_show_progress`,
   frontend `episodeMarks` util + `EpisodeCard` bar / "Watched" badge. Tests: store query, util boundaries
   (0, 79, 80, 100 %, zero duration), card rendering.
2. **Previous / next / auto next.** `findPreviousEpisode` and ordered lookup, `usePlayback` keeps the current
   episode, Previous/Next buttons in `PlayerOverlay`, auto advance with the guards above, the setting.
3. **Skip segments.** `LoadDetails.sync_data` -> `ExternalIds`, new crate `skip_times` (fetcher trait, three
   sources, cache, failure = no segments), command `get_skip_segments`, hook + "Skip Intro" / "Next Episode"
   button, setting. Tests with a fake fetcher, recorded real replies, window boundaries.

## Not in scope

mpv chapter titles as a second source of skip times (few web streams carry chapters); AnimeSkip (needs a key).
