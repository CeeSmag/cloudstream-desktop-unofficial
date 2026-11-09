# Wasm extension runtime spike — findings (2026-11-02)

> **Historical.** This spike (`spikes/`) evaluated running extensions as Kotlin/Wasm in Wasmtime. The route was not
> taken: the app runs real `.cs3` plugins in a JVM sidecar (see the addendum at the end and `AGENTS.md`). The code and
> these notes are kept only as reference.

Setup: Kotlin 2.4.0 `wasmWasi` plugin, Wasmtime 50.0.1 host (GC + exceptions on), JDK 25.

## Proven (code in `spikes/`)
| Capability | Result |
|---|---|
| Kotlin → Wasm GC runs in Wasmtime | works (plain and optimized builds) |
| Host import / plugin export round trip | works |
| Strings both ways (pull-style ABI: plugin passes scratch pointer, host fills) | works |
| DOM via host handles, Jsoup-shaped `Element.select/text/attr` proxy (Rust `scraper` behind it) | works |
| `kotlinx.serialization` JSON inside the plugin | works (has a wasi variant) |
| `suspend` / coroutines (`kotlinx-coroutines-core`) with synchronous host calls | works |
| Handled failure (host refuses request → plugin returns error) | works |
| Uncaught Kotlin exception in plugin | host receives an error, stays alive, next call works |

## Blockers / costs
1. **ksoup and ktor-http publish no `wasmWasi` variant** (only wasmJs). HTML parsing and HTTP must be host functions; extension-facing `Document`/`Element` become proxies. Rust-side `scraper` selector semantics must be proven equal to Jsoup (T3 parity tests).
2. **Jackson is the biggest compatibility gap.** ~100 library files and nearly every extension use `@JsonProperty` + reflection-based `parseJson<T>()`. Wasm has no reflection; `kotlinx.serialization` needs `@Serializable` at compile time. "Recompile unchanged" fails for any extension that parses JSON into plain data classes unless we ship (a) a shim `@JsonProperty`, and (b) a compiler plugin / KSP step that generates serializers for such classes. Needs its own spike.
3. `library` declares only `android()` and `jvm()` targets; its `webMain` folder is not wired to any target. Rhino (JS engine for `JsInterpreter`-adjacent code) and NewPipeExtractor are JVM-only.
4. Gradle on JDK 25 crashes the JVM (C2). Workaround: `-XX:TieredStopAtLevel=1` in `gradle.properties`.
5. Host `scraper::Html` is `!Send`; production host needs its own Send-safe DOM store (e.g. parse once on a dedicated thread/actor).

## Not yet tested
Real HTTP through the host (cookies, redirects, Cloudflare), binary bodies, large payloads, multiple plugins per store, plugin memory limits/timeouts (fuel/epoch), a real CloudStream extension.

## Next spikes (recommended order)
1. Jackson shim + serializer-generation feasibility (decides the "100% recompile" claim).
2. Port a thin `MainAPI` subset to `wasmWasi` and compile one real provider from the extension repos.
3. Resource limits (epoch interruption, memory cap) for crash/hang isolation.

## Addendum: errorcode26/CS3-desktop-client-unofficial (cloned to C:\cs3d, GPL-3.0)
- Runs real `.cs3` by converting DEX to JVM bytecode in-process (dex2jar `dex-tools` 2.4.38 plus custom low-memory passes) and faking Android in an `android-stubs` module. No Wasm, no reimplementation of the extension API: the original `library` is a git submodule.
- Verified here: `:plugin-runtime:test` passes (50 tests) on JDK 25; `:sandbox:run <folder>` converted third-party `MegaProvider.cs3` (6,929 B, self-similarity/MegaRepo) to a JVM jar and reported 100% of its Android API references resolved against the stubs. That path is static analysis only; it did not execute plugin code.
- Gotchas: needs git `core.longpaths` or a short clone path on Windows; `:sandbox:run` passes `-Djava.security.manager=allow`, which JDK 24+ rejects, so use JDK 21 (IntelliJ JBR 21 worked). README's "official" extensions URL (recloudstream/cloudstream-extensions) now returns 404.
- Not verified: dynamic `search()`/`load()` against a real site, Cloudflare/WebView behavior, the claimed AppContainer sandbox (not found in code).
- Implication: the compatibility route is DEX->JVM + Android stubs behind our `ExtensionRuntime` trait (JVM sidecar). Wasm stays an optional second backend.
