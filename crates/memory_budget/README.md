# memory_budget

Memory limits for plugins, video and storage, resolved from the PC's RAM, a profile and the
viewer's own choices. Pure: no I/O, no logging, no dependencies (see `docs/MEMORY.md`).

- **Layers:** `preset of the tier <- the viewer's overrides` -> one immutable `MemoryBudget`.
- **Tier:** `MemoryTier::detect(&dyn SystemProbe)` (Low-end / Balanced / High-end from RAM; unknown RAM counts as low-end). The real probe lives in `app_shell`; tests use `FixedSystem`.
- **Profile:** `ProfileChoice` (Auto or a named tier); "Custom" is any profile with `BudgetOverrides`.
- **Values:** only offered choices count (`budget_choices.rs`); anything else falls back to the preset.
- **Warning:** `estimate_worst_case_mib` + `is_heavy` (more than half of the RAM), never a limit.
- **Not here:** poster/picture limits are fixed for every PC (`ui_slint::poster_limits`).
- **Adding a limit:** a field in `MemoryBudget` and `BudgetOverrides`, a choice list, a value per tier in `budget_presets.rs`, and its Settings control, in the same change as the behavior it limits.
- **Test:** `cargo test -p memory_budget`
- **Rules:** follow the workspace `RULES.md`.
