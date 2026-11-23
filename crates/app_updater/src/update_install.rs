//! Putting a checked update in place. The running program cannot replace itself, so a helper script
//! (`update_helper_script.rs`) waits for the app to exit and then either moves the new file over the
//! old one (portable exe, AppImage) or runs the downloaded installer (installed Windows app).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::update_error::UpdateError;

/// How the downloaded file replaces the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InstallMethod {
    /// Move the download over this file (portable exe, AppImage, or the running program elsewhere).
    ReplaceFile(PathBuf),
    /// Run the downloaded Windows installer, which replaces the installed app and starts it. Moving
    /// the installer over the app's exe would leave the installer running under the app's name,
    /// trying to replace itself.
    RunInstaller,
}

/// What the updater reads about the process it runs in: environment variables and where the
/// running program is. The app uses [`AppProcess::current`]; tests describe one by hand.
pub(crate) struct AppProcess<'a> {
    /// An environment variable's value.
    pub(crate) env: &'a dyn Fn(&str) -> Option<OsString>,
    /// Where the running program is.
    pub(crate) exe: &'a dyn Fn() -> std::io::Result<PathBuf>,
}

impl AppProcess<'static> {
    /// This process.
    pub(crate) fn current() -> Self {
        Self {
            env: &|name| std::env::var_os(name),
            exe: &std::env::current_exe,
        }
    }
}

/// True for the portable exe (`CS_PORTABLE_EXE`), an AppImage (`APPIMAGE`) or a copy with
/// `portable.txt` beside it; these update to the portable file instead of the installer.
pub(crate) fn is_portable_environment() -> bool {
    is_portable(&AppProcess::current())
}

fn is_portable(process: &AppProcess<'_>) -> bool {
    (process.env)("CS_PORTABLE_EXE").is_some()
        || (process.env)("APPIMAGE").is_some()
        || (process.exe)()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|folder| folder.join("portable.txt").exists())
            })
            .unwrap_or(false)
}

fn check_target_writable(target: &Path) -> Result<(), UpdateError> {
    let Some(folder) = target.parent() else {
        return Ok(());
    };
    let probe = folder.join(format!(".cs_update_test_{}", std::process::id()));
    if std::fs::write(&probe, b"").is_err() {
        return Err(UpdateError::ReadOnlyTarget(folder.to_path_buf()));
    }
    if let Err(error) = std::fs::remove_file(&probe) {
        // An empty file left beside the program does no harm; the update can go on.
        tracing::warn!(path = %probe.display(), %error, "cannot remove the write-test file");
    }
    Ok(())
}

fn existing_path_from_env(process: &AppProcess<'_>, name: &str) -> Option<PathBuf> {
    (process.env)(name)
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

/// The file a portable update replaces: the portable exe or (on Linux) the AppImage the app was
/// started from, else the running program. Fails when its folder cannot be written to.
fn target_executable_path(process: &AppProcess<'_>, linux: bool) -> Result<PathBuf, UpdateError> {
    let wrapper = existing_path_from_env(process, "CS_PORTABLE_EXE").or_else(|| {
        linux
            .then(|| existing_path_from_env(process, "APPIMAGE"))
            .flatten()
    });
    let target = match wrapper {
        Some(path) => path,
        None => (process.exe)().map_err(UpdateError::RunningProgramUnknown)?,
    };
    check_target_writable(&target)?;
    Ok(target)
}

/// The method for a system: the installed Windows app runs the installer (and never needs its own
/// folder to be writable: the installer handles that); everything else replaces `target()`.
fn choose_install_method(
    is_windows: bool,
    is_portable: bool,
    target: impl FnOnce() -> Result<PathBuf, UpdateError>,
) -> Result<InstallMethod, UpdateError> {
    if is_windows && !is_portable {
        return Ok(InstallMethod::RunInstaller);
    }
    target().map(InstallMethod::ReplaceFile)
}

/// What this copy of the app is, as far as updating goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Installation {
    /// How the downloaded file replaces the app.
    pub(crate) method: InstallMethod,
    /// True when it takes the portable file of a release instead of the installer.
    pub(crate) portable: bool,
}

impl Installation {
    /// This copy of the app. Its method matches the file `update_assets::select_platform_asset`
    /// picks: the installer for an installed Windows app, the portable file otherwise.
    pub(crate) fn of_this_app() -> Result<Self, UpdateError> {
        Self::of(&AppProcess::current())
    }

    /// The copy of the app `process` describes, on this system.
    fn of(process: &AppProcess<'_>) -> Result<Self, UpdateError> {
        let portable = is_portable(process);
        let method = choose_install_method(cfg!(windows), portable, || {
            target_executable_path(process, cfg!(target_os = "linux"))
        })?;
        Ok(Self { method, portable })
    }
}

/// Writes the helper script for this system into `folder` (the update attempt's own folder) and
/// returns where it is. `pid` is the program the helper waits for before it replaces anything.
#[cfg(windows)]
pub(crate) fn write_install_helper(
    downloaded: &Path,
    method: &InstallMethod,
    folder: &Path,
    pid: u32,
) -> Result<PathBuf, UpdateError> {
    use crate::update_helper_script::{
        windows_installer_script, windows_replace_script, BAT_SCRIPT_NAME,
    };
    let text = match method {
        InstallMethod::ReplaceFile(target) => windows_replace_script(pid, downloaded, target)?,
        InstallMethod::RunInstaller => windows_installer_script(pid, downloaded)?,
    };
    let script = folder.join(BAT_SCRIPT_NAME);
    std::fs::write(&script, text).map_err(|error| UpdateError::disk("write", &script, error))?;
    Ok(script)
}

/// Writes the helper script for this system into `folder` (the update attempt's own folder) and
/// returns where it is. `pid` is the program the helper waits for before it replaces anything.
#[cfg(not(windows))]
pub(crate) fn write_install_helper(
    downloaded: &Path,
    method: &InstallMethod,
    folder: &Path,
    pid: u32,
) -> Result<PathBuf, UpdateError> {
    use crate::update_helper_script::{unix_replace_script, SH_SCRIPT_NAME};
    // An installer is chosen only on Windows; here there is no file this system can install.
    let InstallMethod::ReplaceFile(target) = method else {
        return Err(UpdateError::NoAsset);
    };
    let text = unix_replace_script(pid, downloaded, target)?;
    let script = folder.join(SH_SCRIPT_NAME);
    std::fs::write(&script, text).map_err(|error| UpdateError::disk("write", &script, error))?;
    Ok(script)
}

/// Writes the helper script into `folder` and starts it detached; it waits for this program to
/// exit. The caller exits the app right after.
pub(crate) fn start_install_helper(
    downloaded: &Path,
    method: &InstallMethod,
    folder: &Path,
) -> Result<(), UpdateError> {
    write_install_helper(downloaded, method, folder, std::process::id())?;
    spawn_helper(folder)
}

#[cfg(windows)]
fn spawn_helper(folder: &Path) -> Result<(), UpdateError> {
    use crate::update_helper_script::bat_command;
    use std::os::windows::process::CommandExt;
    /// Windows process flag: run the console helper without showing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    bat_command(folder)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(UpdateError::Spawn)?;
    Ok(())
}

/// No permission change needed: the script is run as `sh <script>`, not executed directly.
#[cfg(not(windows))]
fn spawn_helper(folder: &Path) -> Result<(), UpdateError> {
    use crate::update_helper_script::sh_command;
    sh_command(folder).spawn().map_err(UpdateError::Spawn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_exe() -> Result<PathBuf, UpdateError> {
        Ok(PathBuf::from(
            r"C:\Users\x\AppData\Local\CloudStream\CloudStream.exe",
        ))
    }

    fn unwritable() -> Result<PathBuf, UpdateError> {
        Err(UpdateError::ReadOnlyTarget(PathBuf::from(
            r"C:\Program Files",
        )))
    }

    #[test]
    fn choose_install_method_installed_windows_app_runs_the_installer() {
        let method = choose_install_method(true, false, app_exe).unwrap();
        assert_eq!(method, InstallMethod::RunInstaller);
    }

    #[test]
    fn choose_install_method_installed_windows_app_does_not_need_a_writable_folder() {
        let method = choose_install_method(true, false, unwritable).unwrap();
        assert_eq!(method, InstallMethod::RunInstaller);
    }

    #[test]
    fn choose_install_method_portable_windows_app_replaces_its_exe() {
        let method = choose_install_method(true, true, app_exe).unwrap();
        assert_eq!(method, InstallMethod::ReplaceFile(app_exe().unwrap()));
    }

    #[test]
    fn choose_install_method_linux_replaces_the_file_and_reports_a_read_only_folder() {
        let method = choose_install_method(false, true, app_exe).unwrap();
        assert_eq!(method, InstallMethod::ReplaceFile(app_exe().unwrap()));
        let result = choose_install_method(false, false, unwritable);
        assert!(matches!(result, Err(UpdateError::ReadOnlyTarget(_))));
    }

    /// A folder of its own, removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("cs_update_install_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn check_target_writable_a_writable_folder_passes_and_leaves_no_test_file() {
        let folder = Scratch::new("writable");
        let target = folder.0.join("cloudstream");
        check_target_writable(&target).unwrap();
        let left: Vec<_> = std::fs::read_dir(&folder.0).unwrap().collect();
        assert!(left.is_empty(), "the write test cleans up after itself");
    }

    #[test]
    fn check_target_writable_a_missing_folder_is_read_only() {
        let folder = Scratch::new("missing");
        let target = folder.0.join("not-there").join("cloudstream");
        assert!(matches!(
            check_target_writable(&target),
            Err(UpdateError::ReadOnlyTarget(path)) if path == folder.0.join("not-there")
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn write_install_helper_writes_the_replace_script_into_the_attempts_folder() {
        use crate::update_helper_script::{unix_replace_script, SH_SCRIPT_NAME};
        let folder = Scratch::new("helper");
        let downloaded = folder.0.join("CloudStream.AppImage");
        let target = PathBuf::from("/opt/cloudstream/CloudStream.AppImage");
        let method = InstallMethod::ReplaceFile(target.clone());

        let script = write_install_helper(&downloaded, &method, &folder.0, 4242).unwrap();

        assert_eq!(script, folder.0.join(SH_SCRIPT_NAME));
        assert_eq!(
            std::fs::read_to_string(&script).unwrap(),
            unix_replace_script(4242, &downloaded, &target).unwrap()
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn write_install_helper_an_installer_has_nothing_to_install_here() {
        let folder = Scratch::new("installer");
        let result = write_install_helper(
            &folder.0.join("setup.exe"),
            &InstallMethod::RunInstaller,
            &folder.0,
            1,
        );
        assert!(matches!(result, Err(UpdateError::NoAsset)));
        assert_eq!(
            std::fs::read_dir(&folder.0).unwrap().count(),
            0,
            "no script was written"
        );
    }

    #[cfg(windows)]
    #[test]
    fn write_install_helper_writes_the_installer_script_into_the_attempts_folder() {
        use crate::update_helper_script::{windows_installer_script, BAT_SCRIPT_NAME};
        let folder = Scratch::new("helper");
        let installer = folder.0.join("CloudStream_99.0.0_x64-setup.exe");

        let script =
            write_install_helper(&installer, &InstallMethod::RunInstaller, &folder.0, 4242)
                .unwrap();

        assert_eq!(script, folder.0.join(BAT_SCRIPT_NAME));
        assert_eq!(
            std::fs::read_to_string(&script).unwrap(),
            windows_installer_script(4242, &installer).unwrap()
        );
    }

    #[cfg(windows)]
    #[test]
    fn write_install_helper_writes_the_replace_script_for_a_portable_copy() {
        use crate::update_helper_script::windows_replace_script;
        let folder = Scratch::new("portable");
        let downloaded = folder.0.join("CloudStream_portable.exe");
        let target = folder.0.join("CloudStream.exe");

        let script = write_install_helper(
            &downloaded,
            &InstallMethod::ReplaceFile(target.clone()),
            &folder.0,
            7,
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(script).unwrap(),
            windows_replace_script(7, &downloaded, &target).unwrap()
        );
    }

    /// A process with only the environment variables in `vars` and its program at `exe`.
    fn with_process<T>(
        vars: &[(&str, &Path)],
        exe: std::io::Result<PathBuf>,
        body: impl FnOnce(&AppProcess<'_>) -> T,
    ) -> T {
        let vars: Vec<(String, OsString)> = vars
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.as_os_str().to_owned()))
            .collect();
        let env = move |name: &str| {
            vars.iter()
                .find(|(known, _)| known == name)
                .map(|(_, value)| value.clone())
        };
        let exe = std::cell::RefCell::new(Some(exe));
        let exe = || {
            exe.borrow_mut()
                .take()
                .unwrap_or_else(|| Err(std::io::Error::other("asked twice")))
        };
        body(&AppProcess {
            env: &env,
            exe: &exe,
        })
    }

    #[test]
    fn is_portable_portable_exe_appimage_or_marker_file_and_otherwise_not() {
        let folder = Scratch::new("portable_kinds");
        let exe = folder.0.join("cloudstream");
        let nowhere = Path::new("/x");
        assert!(with_process(
            &[("CS_PORTABLE_EXE", nowhere)],
            Ok(exe.clone()),
            is_portable
        ));
        assert!(with_process(
            &[("APPIMAGE", nowhere)],
            Ok(exe.clone()),
            is_portable
        ));
        assert!(
            !with_process(&[], Ok(exe.clone()), is_portable),
            "an installed copy"
        );
        std::fs::write(folder.0.join("portable.txt"), b"").unwrap();
        assert!(
            with_process(&[], Ok(exe), is_portable),
            "portable.txt beside the program"
        );
        assert!(!with_process(
            &[],
            Err(std::io::Error::other("unknown")),
            is_portable
        ));
    }

    #[test]
    fn target_executable_path_prefers_the_portable_exe_then_the_appimage_then_the_program() {
        let folder = Scratch::new("targets");
        let (portable, appimage, program) = (
            folder.0.join("CloudStream_portable.exe"),
            folder.0.join("CloudStream.AppImage"),
            folder.0.join("cloudstream"),
        );
        for file in [&portable, &appimage] {
            std::fs::write(file, b"").unwrap();
        }
        let both = [
            ("CS_PORTABLE_EXE", portable.as_path()),
            ("APPIMAGE", appimage.as_path()),
        ];
        let target = |vars: &[(&str, &Path)], linux| {
            with_process(vars, Ok(program.clone()), |process| {
                target_executable_path(process, linux).unwrap()
            })
        };
        assert_eq!(target(&both, true), portable);
        assert_eq!(target(&both[1..], true), appimage);
        assert_eq!(
            target(&both[1..], false),
            program,
            "APPIMAGE counts on Linux only"
        );
        assert_eq!(target(&[], true), program);
    }

    #[test]
    fn target_executable_path_a_variable_naming_a_missing_file_is_ignored() {
        let folder = Scratch::new("missing_target");
        let program = folder.0.join("cloudstream");
        let gone = folder.0.join("moved-away.exe");
        let target = with_process(
            &[("CS_PORTABLE_EXE", gone.as_path())],
            Ok(program.clone()),
            |p| target_executable_path(p, true).unwrap(),
        );
        assert_eq!(target, program);
    }

    #[test]
    fn target_executable_path_an_unknown_program_is_reported() {
        let result = with_process(&[], Err(std::io::Error::other("unknown")), |process| {
            target_executable_path(process, true)
        });
        assert!(matches!(result, Err(UpdateError::RunningProgramUnknown(_))));
    }

    #[test]
    fn installation_of_a_portable_copy_replaces_its_own_file() {
        let folder = Scratch::new("installation");
        let portable = folder.0.join("CloudStream_portable.exe");
        std::fs::write(&portable, b"").unwrap();
        let installation = with_process(
            &[("CS_PORTABLE_EXE", portable.as_path())],
            Ok(folder.0.join("cloudstream")),
            |process| Installation::of(process).unwrap(),
        );
        assert_eq!(
            installation,
            Installation {
                method: InstallMethod::ReplaceFile(portable),
                portable: true
            }
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn spawn_helper_runs_the_script_in_the_attempts_folder() {
        use crate::update_helper_script::SH_SCRIPT_NAME;
        let folder = Scratch::new("spawn");
        std::fs::write(folder.0.join(SH_SCRIPT_NAME), "echo ran > ran.txt\n").unwrap();
        spawn_helper(&folder.0).unwrap();
        let marker = folder.0.join("ran.txt");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(marker.exists(), "the helper script was not run");
    }

    #[cfg(not(windows))]
    #[test]
    fn start_install_helper_starts_nothing_when_no_script_can_be_written() {
        let folder = Scratch::new("start_nothing");
        let result = start_install_helper(
            &folder.0.join("setup.exe"),
            &InstallMethod::RunInstaller,
            &folder.0,
        );
        assert!(matches!(result, Err(UpdateError::NoAsset)));
    }
}
