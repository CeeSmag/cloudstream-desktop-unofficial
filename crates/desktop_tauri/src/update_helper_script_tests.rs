//! Tests of the install helper scripts: T1 on the text, and T2 runs of the real script with odd
//! folder names (the `sh` one runs on Linux/macOS and, with `--ignored`, under Git Bash; the `.bat`
//! one is ignored because it starts a program in a new window).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::*;

/// A folder name with every character that broke the old unquoted scripts.
const ODD_FOLDER: &str = "cs helper a&b %PATH% ^c 'd' $HOME `e`";

fn position(script: &str, needle: &str) -> usize {
    script
        .find(needle)
        .unwrap_or_else(|| panic!("missing {needle:?} in:\n{script}"))
}

#[test]
fn bat_quote_percent_is_doubled_so_cmd_keeps_it() {
    let quoted = bat_quote(Path::new(r"C:\Users\a%b%\Cloud Stream.exe")).unwrap();
    assert_eq!(quoted, r#""C:\Users\a%%b%%\Cloud Stream.exe""#);
}

#[test]
fn bat_quote_ampersand_and_caret_stay_inside_quotes() {
    let quoted = bat_quote(Path::new(r"C:\a&b^c\x.exe")).unwrap();
    assert_eq!(quoted, r#""C:\a&b^c\x.exe""#);
}

#[test]
fn bat_quote_path_with_double_quote_returns_unsafe_path() {
    let result = bat_quote(Path::new("C:\\a\"b\\x.exe"));
    assert!(matches!(result, Err(UpdateError::UnsafePath(_))));
}

#[test]
fn sh_quote_quote_dollar_and_backtick_stay_literal() {
    let quoted = sh_quote(Path::new("/home/o'neil/$HOME/`x`")).unwrap();
    assert_eq!(quoted, r"'/home/o'\''neil/$HOME/`x`'");
}

#[test]
fn windows_replace_script_for_pid_waits_then_moves_then_starts() {
    let script =
        windows_replace_script(42, Path::new(r"C:\u\new.exe"), Path::new(r"C:\a%x\cs.exe"))
            .unwrap();
    let wait = position(&script, "PID eq 42");
    let swap = position(&script, r#"move /y "C:\u\new.exe" "C:\a%%x\cs.exe""#);
    let start = position(&script, r#"start "" "C:\a%%x\cs.exe""#);
    assert!(wait < swap && swap < start);
}

#[test]
fn windows_installer_script_for_pid_waits_then_runs_installer_as_update() {
    let installer = Path::new(r"C:\data\updates\1-2\CloudStream_0.5.0_x64-setup.exe");
    let script = windows_installer_script(42, installer).unwrap();
    let wait = position(&script, "PID eq 42");
    let run = position(
        &script,
        r#"start "" /wait "C:\data\updates\1-2\CloudStream_0.5.0_x64-setup.exe" /P /R /UPDATE"#,
    );
    assert!(wait < run);
    assert!(
        !script.contains("move"),
        "the installer must not be moved over the app"
    );
}

#[test]
fn unix_replace_script_for_pid_waits_then_moves_then_starts() {
    let script =
        unix_replace_script(42, Path::new("/tmp/new"), Path::new("/opt/it's.AppImage")).unwrap();
    let wait = position(&script, "kill -0 42");
    let swap = position(&script, r"mv -f '/tmp/new' '/opt/it'\''s.AppImage'");
    let start = position(&script, r"nohup '/opt/it'\''s.AppImage'");
    assert!(wait < swap && swap < start);
}

/// A process id that has already ended.
fn finished_pid(program: &str, args: &[&str]) -> u32 {
    let mut child = Command::new(program).args(args).spawn().unwrap();
    child.wait().unwrap();
    child.id()
}

/// A fresh folder with `ODD_FOLDER` in its name, one per test (`test` keeps parallel tests apart).
fn odd_folder(test: &str) -> PathBuf {
    let name = format!("{ODD_FOLDER} {test} {}", std::process::id());
    let folder = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn wait_for(path: &Path) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
#[cfg_attr(
    not(unix),
    ignore = "runs the helper with sh; on Windows run with --ignored and Git Bash's sh on PATH"
)]
fn unix_replace_script_odd_folder_moves_and_starts_the_file() {
    let folder = odd_folder("sh-odd");
    let downloaded = folder.join("new version");
    let target = folder.join("Cloud'Stream $x.AppImage");
    std::fs::write(
        &downloaded,
        "#!/bin/sh\necho started > \"$(dirname \"$0\")/started\"\n",
    )
    .unwrap();
    std::fs::write(&target, "old").unwrap();
    let script = folder.join(SH_SCRIPT_NAME);
    let pid = finished_pid("sh", &["-c", "exit 0"]);
    std::fs::write(
        &script,
        unix_replace_script(pid, &downloaded, &target).unwrap(),
    )
    .unwrap();

    let status = sh_command(&folder).status().unwrap();

    assert!(status.success());
    assert!(!downloaded.exists());
    assert!(std::fs::read_to_string(&target)
        .unwrap()
        .contains("echo started"));
    assert!(
        wait_for(&folder.join("started")),
        "the new file was not started"
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
#[cfg(unix)]
fn unix_replace_script_running_app_waits_until_it_ends() {
    let folder = odd_folder("sh-wait");
    let downloaded = folder.join("new");
    let target = folder.join("old");
    std::fs::write(
        &downloaded,
        "#!/bin/sh
",
    )
    .unwrap();
    std::fs::write(&target, "old").unwrap();
    let mut app = Command::new("sleep").arg("1").spawn().unwrap();
    let script_text = unix_replace_script(app.id(), &downloaded, &target).unwrap();
    std::fs::write(folder.join(SH_SCRIPT_NAME), script_text).unwrap();

    let status = sh_command(&folder).status().unwrap();

    assert!(status.success());
    assert!(
        app.try_wait().unwrap().is_some(),
        "the helper did not wait for the app to end"
    );
    assert!(!downloaded.exists());
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
#[cfg(windows)]
#[ignore = "starts a program in a new console window; run with --ignored"]
fn windows_replace_script_odd_folder_moves_and_starts_the_file() {
    let folder = odd_folder("bat-odd");
    let downloaded = folder.join("new version.exe");
    let target = folder.join("Cloud%Stream%&^.exe");
    std::fs::copy(r"C:\Windows\System32\hostname.exe", &downloaded).unwrap();
    std::fs::write(&target, "old").unwrap();
    let script = folder.join(BAT_SCRIPT_NAME);
    // The app stands in as a process that is still running for about two seconds.
    let mut app = Command::new(r"C:\Windows\System32\PING.EXE")
        .args(["-n", "3", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let script_text = windows_replace_script(app.id(), &downloaded, &target).unwrap();
    std::fs::write(&script, script_text).unwrap();

    let status = bat_command(&folder).status().unwrap();

    assert!(status.success());
    assert!(
        app.try_wait().unwrap().is_some(),
        "the helper did not wait for the app to end"
    );
    assert!(!downloaded.exists(), "the download was not moved");
    assert!(
        std::fs::metadata(&target).unwrap().len() > 3,
        "the target was not replaced"
    );
    assert!(!script.exists(), "the script did not remove itself");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
#[cfg(windows)]
#[ignore = "starts a program in a new console window; run with --ignored"]
fn windows_installer_script_odd_folder_waits_runs_and_removes_the_installer() {
    let folder = odd_folder("bat-installer");
    // whoami.exe stands in for the installer: it ends at once, whatever /P /R /UPDATE mean to it.
    let installer = folder.join("CloudStream_0.5.0_x64-setup.exe");
    std::fs::copy(r"C:\Windows\System32\whoami.exe", &installer).unwrap();
    let mut app = Command::new(r"C:\Windows\System32\PING.EXE")
        .args(["-n", "3", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let script = folder.join(BAT_SCRIPT_NAME);
    std::fs::write(
        &script,
        windows_installer_script(app.id(), &installer).unwrap(),
    )
    .unwrap();

    let status = bat_command(&folder).status().unwrap();

    assert!(status.success());
    assert!(
        app.try_wait().unwrap().is_some(),
        "the helper did not wait for the app to end"
    );
    assert!(
        !installer.exists(),
        "the installer was not removed after it ran"
    );
    assert!(!script.exists(), "the script did not remove itself");
    let _ = std::fs::remove_dir_all(&folder);
}
