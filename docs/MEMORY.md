# Memory budget and measurements

Target: a **4 GB PC** with the whole app (every process it starts) at or under the budgets below.
Why a tree: the app is `cloudstream` + one `java` sidecar per resident plugin + the Cloudflare helper
and its WebKit/WebView2 children. One process alone hides most of the cost.

## Budgets (acceptance, total process-tree PSS)

| Scenario | Budget |
|---|---|
| S1 idle Home, 1 resident plugin | <= 450 MiB |
| S3 playing 1080p HLS | <= 700 MiB |

Other scenarios are tracked but have no budget yet; set one once a baseline exists.

## How to measure

```bash
# Follow a running app (or start it) and report the peak of the whole tree.
python3 scripts/measure_memory.py --launch --label S1 --duration 120 -- ./cloudstream
python3 scripts/measure_memory.py --pid <pid> --label S3 --duration 600 --max-total-mib 700
just memory --pid <pid> --label S1          # same script through just
```

Linux reports PSS (shared pages counted once, `/proc/<pid>/smaps_rollup`). Windows reports working
set, an upper bound. Do not compare the two OS numbers with each other.

Use a **scratch data directory** (`CS_DATA_DIR=$(mktemp -d)`), never the real one: measuring must not
touch the viewer's database, plugins or history.

For the JVM breakdown, run a measurement build with `-XX:NativeMemoryTracking=summary` and
`jcmd <pid> VM.native_memory summary`. To settle software versus hardware decoding, read
`hwdec-current` from `logs/debug/player-logs.jsonl` (logged once, 30 s into a stream).

## Scenarios

| ID | What |
|---|---|
| S1 | Cold start to idle Home, 5+ plugins installed, upgraded from an "unlimited" install |
| S2 | Switch provider twice |
| S3 | 1080p HLS for 10 minutes |
| S4 | 4K progressive file for 5 minutes |
| S5 | Browse, 3 pages, then Back |
| S6 | Leave the player |

## Baseline

Not recorded yet. Every number in the optimization plan is an estimate until this table is filled.

| Scenario | Peak total MiB | JVMs | Notes |
|---|---|---|---|
| S1 | | | |
| S2 | | | |
| S3 | | | |
| S4 | | | |
| S5 | | | |
| S6 | | | |

## Decision log

One entry per optimization step: what changed, the alternative considered, the measured delta,
and the switch that turns it back off. A step stays only if its delta is real.

| Step | Change | Measured delta | Rollback switch |
|---|---|---|---|
| (none yet) | | | |
