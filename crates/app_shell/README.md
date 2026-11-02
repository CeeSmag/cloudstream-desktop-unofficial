# app_shell

The `cloudstream` executable. It contains no logic of its own: it finds the data folder, starts
logging, discovers the JVM sidecar and libmpv, builds the services and hands them to `ui_app`.

Everything it needs is discovered, and a missing piece degrades instead of crashing:

| Missing | Result |
|---|---|
| Sidecar (`CS_SIDECAR_HOME`) | App starts; extension calls report "runtime not available" |
| libmpv (`CS_LIBMPV_PATH`) | App starts; Play shows why playback is unavailable |

Environment variables: `CS_DATA_DIR`, `CS_SIDECAR_HOME`, `CS_SIDECAR_JAVA_HOME`, `CS_LIBMPV_PATH`,
`RUST_LOG`.
