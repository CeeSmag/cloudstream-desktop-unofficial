use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn validate_wipe_target_allows_portable_runtime_inside_data_dir() {
    let data = Path::new("/users/alice/CloudStreamRs");
    let portable = Path::new("/users/alice/Desktop/CloudStream_0.4.8_x64_portable.exe");
    let runtime_exe = data.join("runtime-0.4.8-abcd").join("cloudstream.exe");
    assert!(validate_wipe_target(data, Some(portable), Some(&runtime_exe)).is_ok());
}

#[test]
fn validate_wipe_target_allows_installed_and_folder_portable_layouts() {
    let data = Path::new("/users/alice/CloudStreamRs");
    let installed_exe = Path::new("/Program Files/CloudStream/cloudstream.exe");
    assert!(validate_wipe_target(data, None, Some(installed_exe)).is_ok());

    let folder_root = Path::new("/apps/CloudStream");
    let folder_data = folder_root.join("data");
    let folder_exe = folder_root.join("cloudstream.exe");
    assert!(validate_wipe_target(&folder_data, None, Some(&folder_exe)).is_ok());
}

#[test]
fn validate_wipe_target_refuses_drive_root_and_program_folders() {
    let root = PathBuf::from(std::path::MAIN_SEPARATOR_STR);
    assert!(validate_wipe_target(&root, None, None).is_err());

    let desktop = Path::new("/users/alice/Desktop");
    let portable = desktop.join("CloudStream_0.4.8_x64_portable.exe");
    assert!(validate_wipe_target(desktop, Some(&portable), None).is_err());

    let install_dir = Path::new("/Program Files/CloudStream");
    let installed_exe = install_dir.join("cloudstream.exe");
    assert!(validate_wipe_target(install_dir, None, Some(&installed_exe)).is_err());
}

#[test]
fn windows_wipe_script_escapes_percent_and_quotes_special_paths() {
    let dir = Path::new(r"C:\Users\a & b %TEMP%\AppData\Local\CloudStreamRs");
    let script = windows_wipe_script(4242, dir).expect("script");
    assert!(script.contains("PID eq 4242"));
    assert!(script.contains(r#"rd /s /q "C:\Users\a & b %%TEMP%%\AppData\Local\CloudStreamRs""#));
    assert!(script.contains(r#"(goto) 2>nul & del "%~f0""#));
}

#[test]
fn unix_wipe_script_single_quotes_special_paths() {
    let dir = Path::new("/home/o'neil/.local/share/cloudstream-rs");
    let script = unix_wipe_script(4242, dir).expect("script");
    assert!(script.contains("kill -0 4242"));
    assert!(script.contains(r"rm -rf '/home/o'\''neil/.local/share/cloudstream-rs'"));
    assert!(script.contains(r#"rm -f "$0""#));
}

#[test]
fn validate_wipe_target_refuses_quotes_and_control_characters() {
    assert!(validate_wipe_target(Path::new("/tmp/bad\npath"), None, None).is_err());
    assert!(validate_wipe_target(Path::new(r#"/tmp/bad"path"#), None, None).is_err());
}

fn spawn_short_lived_child() -> std::process::Child {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new("cmd.exe")
            .args(["/d", "/c", "exit 0"])
            .creation_flags(0x0800_0000)
            .spawn()
            .expect("spawn dummy cmd")
    }
    #[cfg(not(windows))]
    {
        Command::new("sh")
            .args(["-c", "exit 0"])
            .spawn()
            .expect("spawn dummy sh")
    }
}

#[test]
fn spawn_wipe_helper_for_pid_removes_entire_data_dir_and_self_deletes() {
    let stamp = format!("cs_wipe_helper_t2_{}", std::process::id());
    let root = std::env::temp_dir().join(stamp);
    let data_dir = root.join("CloudStreamRs");
    let scripts_dir = root.join("scripts");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(data_dir.join("runtime-0.4.8-test")).expect("runtime");
    std::fs::create_dir_all(data_dir.join("logs")).expect("logs");
    std::fs::write(
        data_dir.join("runtime-0.4.8-test").join("cloudstream.exe"),
        b"bin",
    )
    .expect("exe");
    std::fs::write(data_dir.join("cloudstream.db"), b"db").expect("db");

    let mut child = spawn_short_lived_child();
    let pid = child.id();
    let script_path =
        spawn_wipe_helper_for_pid(pid, &data_dir, &scripts_dir).expect("spawn helper");
    let _ = child.wait();

    let deadline = Instant::now() + Duration::from_secs(10);
    while (data_dir.exists() || script_path.exists()) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }

    assert!(
        !data_dir.exists(),
        "wipe helper must remove the entire data dir (including runtime-*)"
    );
    assert!(
        !script_path.exists(),
        "wipe helper script must delete itself after finishing"
    );
    let _ = std::fs::remove_dir_all(&root);
}
