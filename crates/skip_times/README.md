# skip_times

Finds where the intro, recap, credits and preview of an episode or film are.

Providers do not carry these times. The reference app asks community databases by the title's ids
and uses the first that knows; so does this crate. Nothing here touches the player: a caller
passes a `SkipQuery` and gets `SkipSegment`s (or an empty list).

| Title | Order |
|---|---|
| Anime | AniSkip (MyAnimeList id + episode), then TheIntroDB |
| Series | TheIntroDB (TMDb or IMDb id + season + episode), then IntroDB (IMDb id) |
| Film | TheIntroDB |

| Part | Responsibility |
|---|---|
| `ExternalIds::from_sync` | ids from the provider's `sync_data` / `unique_url`; keeps only plain digits (1–10) for TMDb/MAL, or `tt` + 5–12 digits for IMDb |
| `SkipFetcher` / `UreqSkipFetcher` | one HTTP GET: 6 s global / 4 s connect, 256 KB cap, `CloudStream-rs` User-Agent |
| `SkipLookup::find` | asks databases in order until one returns segments; remembers answers for this run (64 titles, oldest dropped); empty answers are remembered, errors are not |
| `clean_segments` | drops backwards/empty/repeated segments and starts past the end; clamps ends past duration; drops stretches covering more than four fifths of the video; keeps at most 8, sorted by start |

Test: `cargo test -p skip_times`. Live databases: `cargo test -p skip_times --test live_lookup -- --ignored`.
