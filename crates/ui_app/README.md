# ui_app

The egui front end. It mirrors the CloudStream layout: bottom navigation (Home, Search, Library,
Downloads, Settings), poster rows, a details page with episodes, and a now-playing bar for mpv.

Data flow (one direction):

```
screens (draw model, emit UiAction) -> actions (spawn work on TaskRunner) -> UiEvent
   -> event_handler (updates AppModel) -> screens
```

- `screens/` and `widgets/` only draw; they never call services.
- `actions/` is the only place that calls `app_services`.
- `task_runner.rs` runs work on threads and turns panics into notices.
- Run the app through `app_shell`.
