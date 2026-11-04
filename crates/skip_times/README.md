# skip_times

Finds where the intro, recap and credits of an episode or film are.

Providers do not carry these times. The reference app asks community databases by the title's ids
and uses the first that knows; so does this crate:

| Title | Order |
|---|---|
| Anime | AniSkip (MyAnimeList id + episode), then TheIntroDB |
| Series | TheIntroDB (TMDb or IMDb id + season + episode), then IntroDB (IMDb id) |
| Film | TheIntroDB |

The ids come from the provider's `syncData` / `uniqueUrl` (see `ExternalIds::from_sync`); an id that
is not plain digits (or `tt` + digits for IMDb) is dropped so it cannot change a request.

- `SkipLookup::find` never blocks the player: every request has a 6 s limit, answers are
  remembered for the run (64 titles at most), an error is never remembered.
- Answers are cleaned (`clean_segments.rs`): backwards, repeated, past-the-end and nearly-whole-video
  segments are dropped.
- `tests/live_lookup.rs` asks the real services; run it with `-- --ignored`.
