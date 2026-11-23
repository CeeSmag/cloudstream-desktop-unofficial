//! T2: the production worker pool (`PluginWorkerPool::new`) against real child processes of the
//! scripted fake sidecar: one process per plugin, the low-end limit of one live process, a parked
//! plugin coming back in a new process, trusted plugins, and shutdown.
#![allow(clippy::expect_used, clippy::indexing_slicing, clippy::panic)] // test code: failing fast is the point

use std::path::Path;

use extension_api::ProviderGateway;
use extension_runtime::{PluginWorkerPool, SidecarLaunchConfig};

fn pool() -> PluginWorkerPool {
    PluginWorkerPool::new(SidecarLaunchConfig {
        program: env!("CARGO_BIN_EXE_fake_sidecar").into(),
        args: Vec::new(),
        environment: Vec::new(),
        os_fs_sandbox: false,
        spawner: None,
    })
}

/// The process id of the worker that answered a search of `provider`.
fn answering_process(pool: &PluginWorkerPool, provider: &str) -> String {
    let page = pool.search(provider, "q", 1).expect("searched");
    let name = &page.items[0].name;
    name.split(':').next().expect("pid:query").to_owned()
}

#[test]
fn each_plugin_runs_in_a_process_of_its_own() {
    let pool = pool();
    pool.set_resident_worker_limit(None);
    let alpha = pool
        .load_extension(Path::new("/ext/Alpha.cs3"), false)
        .expect("loads");
    pool.load_extension(Path::new("/ext/Beta.cs3"), false)
        .expect("loads");
    assert_eq!(alpha[0].name, "Alpha");
    assert_ne!(
        answering_process(&pool, "Alpha"),
        answering_process(&pool, "Beta")
    );
    pool.shutdown();
}

#[test]
fn on_a_low_end_pc_one_plugin_lives_and_a_parked_one_comes_back_in_a_new_process() {
    let pool = pool();
    pool.load_extension(Path::new("/ext/Alpha.cs3"), false)
        .expect("loads");
    let first = answering_process(&pool, "Alpha");

    // The production pool keeps one plugin process: loading Beta stops Alpha's.
    pool.load_extension(Path::new("/ext/Beta.cs3"), false)
        .expect("loads");
    let beta = answering_process(&pool, "Beta");
    let revived = answering_process(&pool, "Alpha");

    assert_ne!(
        revived, first,
        "Alpha was parked, so a new process answered"
    );
    assert_ne!(revived, beta);
    pool.shutdown();
}

#[test]
fn a_trusted_plugin_runs_and_answers_like_any_other() {
    let pool = pool();
    let providers = pool
        .load_extension(Path::new("/ext/Trusted.cs3"), true)
        .expect("loads");
    assert_eq!(providers[0].name, "Trusted");
    assert!(!answering_process(&pool, "Trusted").is_empty());
    pool.shutdown();
}

#[test]
fn after_shutdown_no_plugin_answers() {
    let pool = pool();
    pool.load_extension(Path::new("/ext/Alpha.cs3"), false)
        .expect("loads");
    pool.set_debug_logging(true)
        .expect("the worker takes the switch");
    pool.shutdown();
    assert!(pool.search("Alpha", "q", 1).is_err());
}
