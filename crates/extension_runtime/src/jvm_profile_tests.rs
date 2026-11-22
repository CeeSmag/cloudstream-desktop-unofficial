use std::path::Path;

use super::JvmProfile;
use crate::SidecarLaunchConfig;

fn java_options(config: &SidecarLaunchConfig) -> String {
    config
        .environment
        .iter()
        .find(|(name, _)| name == "JAVA_OPTS")
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

#[test]
fn the_profile_is_read_from_its_name_and_unknown_names_are_refused() {
    assert_eq!(JvmProfile::parse("lean"), Some(JvmProfile::Lean));
    assert_eq!(JvmProfile::parse(" Legacy "), Some(JvmProfile::Legacy));
    assert_eq!(JvmProfile::parse("fast"), None);
    assert_eq!(JvmProfile::default(), JvmProfile::Lean);
}

#[test]
fn legacy_adds_nothing_so_it_is_a_complete_way_back() {
    assert!(JvmProfile::Legacy.java_options().is_empty());
    assert!(JvmProfile::Legacy.environment().is_empty());
}

#[test]
fn lean_uses_the_serial_collector_the_light_compiler_and_bounded_memory() {
    let options = JvmProfile::Lean.java_options();
    for wanted in [
        "-XX:+UseSerialGC",
        "-XX:TieredStopAtLevel=1",
        "-XX:MaxMetaspaceSize=128m",
        "-XX:MaxDirectMemorySize=64m",
        "-XX:+ExitOnOutOfMemoryError",
    ] {
        assert!(options.contains(&wanted), "{wanted} missing");
    }
}

#[test]
fn lean_leaves_the_heap_cap_and_the_class_space_to_the_launch_config() {
    let options = JvmProfile::Lean.java_options();
    assert!(options.iter().all(|option| !option.starts_with("-Xmx")));
    assert!(options
        .iter()
        .all(|option| !option.contains("CompressedClassSpaceSize")));
}

#[test]
fn lean_never_repeats_a_flag() {
    let options = JvmProfile::Lean.java_options();
    let mut names: Vec<&str> = options
        .iter()
        .map(|option| option.split('=').next().unwrap_or(option))
        .collect();
    names.sort_unstable();
    let before = names.len();
    names.dedup();
    assert_eq!(names.len(), before);
}

#[test]
fn the_profile_joins_the_single_java_options_variable() {
    let config = SidecarLaunchConfig::for_install(Path::new("h"), None)
        .with_data_dir(Path::new("/d"))
        .with_jvm_profile(JvmProfile::Lean);
    let options = java_options(&config);
    assert!(
        options.contains("-Xmx512m"),
        "existing options stay: {options}"
    );
    assert!(options.contains("-XX:+UseSerialGC"), "{options}");
    let variables = config
        .environment
        .iter()
        .filter(|(name, _)| name == "JAVA_OPTS")
        .count();
    assert_eq!(variables, 1);
}

#[test]
fn legacy_leaves_the_launch_config_as_it_was() {
    let before =
        SidecarLaunchConfig::for_install(Path::new("h"), None).with_data_dir(Path::new("/d"));
    let after = before.clone().with_jvm_profile(JvmProfile::Legacy);
    assert_eq!(before.environment, after.environment);
}

#[test]
fn malloc_arenas_are_capped_for_the_child_on_linux_only() {
    let config =
        SidecarLaunchConfig::for_install(Path::new("h"), None).with_jvm_profile(JvmProfile::Lean);
    let capped = config
        .environment
        .iter()
        .any(|(name, value)| name == "MALLOC_ARENA_MAX" && value == "2");
    assert_eq!(capped, cfg!(target_os = "linux"));
}
