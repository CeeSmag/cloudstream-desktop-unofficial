# ui_app — REFERENCE ONLY

**Not the product UI.** This egui shell is kept in-tree as a porting reference.
Product builds ship the Slint shell from `crates/ui_slint` (binary `cloudstream` via `app_shell`).

The layout mirrors CloudStream: bottom navigation (Home, Search, Library, Downloads,
Settings), poster rows, a details page with episodes, and a now-playing bar for mpv.

Data flow (one direction):

```
screens (draw model, emit UiAction) -> actions (spawn work on TaskRunner) -> UiEvent
   -> event_handler (updates AppModel) -> screens
```

- `screens/` and `widgets/` only draw; they never call services.
- `actions/` is the only place that calls `app_services`.
- `task_runner.rs` runs work on threads and turns panics into notices.
