//! Landlock filesystem allow-list for Linux sidecar workers (AppContainer analogue).
#![allow(unsafe_code)]

use std::ffi::CString;
use std::io;
use std::os::unix::io::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;

use extension_api::GatewayError;

use crate::sidecar_linux_limits::sandbox_unavailable;
use crate::sidecar_linux_paths::SandboxPaths;

const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
const SYS_LANDLOCK_ADD_RULE: libc::c_long = 445;
const SYS_LANDLOCK_RESTRICT_SELF: libc::c_long = 446;
const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1 << 0;
const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;

const ACCESS_EXECUTE: u64 = 1 << 0;
const ACCESS_WRITE_FILE: u64 = 1 << 1;
const ACCESS_READ_FILE: u64 = 1 << 2;
const ACCESS_READ_DIR: u64 = 1 << 3;
const ACCESS_REMOVE_DIR: u64 = 1 << 4;
const ACCESS_REMOVE_FILE: u64 = 1 << 5;
const ACCESS_MAKE_CHAR: u64 = 1 << 6;
const ACCESS_MAKE_DIR: u64 = 1 << 7;
const ACCESS_MAKE_REG: u64 = 1 << 8;
const ACCESS_MAKE_SOCK: u64 = 1 << 9;
const ACCESS_MAKE_FIFO: u64 = 1 << 10;
const ACCESS_MAKE_BLOCK: u64 = 1 << 11;
const ACCESS_MAKE_SYM: u64 = 1 << 12;
const ACCESS_REFER: u64 = 1 << 13;
const ACCESS_TRUNCATE: u64 = 1 << 14;

const ACCESS_RO: u64 = ACCESS_EXECUTE | ACCESS_READ_FILE | ACCESS_READ_DIR;
const ACCESS_RW_BASE: u64 = ACCESS_RO
    | ACCESS_WRITE_FILE
    | ACCESS_REMOVE_DIR
    | ACCESS_REMOVE_FILE
    | ACCESS_MAKE_CHAR
    | ACCESS_MAKE_DIR
    | ACCESS_MAKE_REG
    | ACCESS_MAKE_SOCK
    | ACCESS_MAKE_FIFO
    | ACCESS_MAKE_BLOCK
    | ACCESS_MAKE_SYM;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
}

#[repr(C)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// Fails closed when the kernel has no usable Landlock ABI.
pub(crate) fn ensure_supported() -> Result<i32, GatewayError> {
    abi_version().map_err(|error| {
        sandbox_unavailable(format!(
            "Landlock unavailable: {error}; need Linux ≥ 5.13 with Landlock enabled"
        ))
    })
}

/// Installs `pre_exec` hooks: process limits then Landlock (async-signal-safe child setup).
pub(crate) fn attach_to_command(command: &mut std::process::Command, paths: SandboxPaths) {
    use std::os::unix::process::CommandExt;

    // SAFETY: closure runs only in the child after fork; only setpgid/prctl/setrlimit/Landlock.
    unsafe {
        command.pre_exec(move || {
            crate::sidecar_linux_limits::apply_in_child()?;
            restrict_in_child(&paths)?;
            Ok(())
        });
    }
}

/// Applies the FS allow-list in the forked child before `exec`.
pub(crate) fn restrict_in_child(paths: &SandboxPaths) -> io::Result<()> {
    let abi = abi_version()?;
    let handled = handled_access(abi);
    let ruleset = create_ruleset(handled)?;
    add_ro(&ruleset, &paths.java_home, handled)?;
    add_ro(&ruleset, &paths.sidecar_home, handled)?;
    add_ro(&ruleset, &paths.extensions_dir, handled)?;
    add_rw(&ruleset, &paths.data_dir, handled, abi)?;
    add_rw(&ruleset, Path::new("/tmp"), handled, abi)?;
    // /dev needs write for `/dev/null` (shell redirects, JVM); urandom stays readable via RW.
    add_rw_if_present(&ruleset, Path::new("/dev"), handled, abi)?;
    // System roots the shell script + JVM need (interpreter, libc, CA certs). Still deny $HOME files.
    for system in ["/bin", "/usr", "/lib", "/lib64", "/etc", "/proc"] {
        add_ro_if_present(&ruleset, Path::new(system), handled)?;
    }
    // Path walks need READ_DIR on every ancestor of the allow-list roots (not READ_FILE).
    for root in [
        paths.java_home.as_path(),
        paths.sidecar_home.as_path(),
        paths.extensions_dir.as_path(),
        paths.data_dir.as_path(),
        Path::new("/tmp"),
    ] {
        add_ancestors_read_dir(&ruleset, root, handled)?;
    }
    // SAFETY: required before landlock_restrict_self; clears ambient privilege raising.
    let no_new = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if no_new != 0 {
        return Err(io::Error::last_os_error());
    }
    restrict_self(&ruleset)?;
    Ok(())
}

fn add_ro_if_present(ruleset: &OwnedFd, path: &Path, handled: u64) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    add_ro(ruleset, path, handled)
}

fn add_rw_if_present(ruleset: &OwnedFd, path: &Path, handled: u64, abi: i32) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    add_rw(ruleset, path, handled, abi)
}

fn add_ancestors_read_dir(ruleset: &OwnedFd, path: &Path, handled: u64) -> io::Result<()> {
    let mut current = path.to_path_buf();
    while let Some(parent) = current.parent() {
        if parent.as_os_str().is_empty() {
            break;
        }
        if parent.exists() {
            add_path(ruleset, parent, ACCESS_READ_DIR & handled)?;
        }
        if parent == Path::new("/") {
            break;
        }
        current = parent.to_path_buf();
    }
    Ok(())
}

fn handled_access(abi: i32) -> u64 {
    let mut access = ACCESS_RW_BASE;
    if abi >= 2 {
        access |= ACCESS_REFER;
    }
    if abi >= 3 {
        access |= ACCESS_TRUNCATE;
    }
    access
}

fn rw_access(abi: i32) -> u64 {
    let mut access = ACCESS_RW_BASE;
    if abi >= 3 {
        access |= ACCESS_TRUNCATE;
    }
    access
}

/// Kernel Landlock ABI number, or an I/O error when Landlock is missing.
pub(crate) fn abi_version() -> io::Result<i32> {
    // SAFETY: kernel probe; null attr + VERSION flag returns the ABI number or -errno.
    let result = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(result as i32)
}

fn create_ruleset(handled: u64) -> io::Result<OwnedFd> {
    let attr = RulesetAttr {
        handled_access_fs: handled,
    };
    // SAFETY: create_ruleset copies attr and returns an owned fd or -errno.
    let result = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            &attr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: positive result is a fresh ruleset fd owned by us.
    Ok(unsafe { OwnedFd::from_raw_fd(result as i32) })
}

fn add_ro(ruleset: &OwnedFd, path: &Path, handled: u64) -> io::Result<()> {
    add_path(ruleset, path, ACCESS_RO & handled)
}

fn add_rw(ruleset: &OwnedFd, path: &Path, handled: u64, abi: i32) -> io::Result<()> {
    add_path(ruleset, path, rw_access(abi) & handled)
}

fn add_path(ruleset: &OwnedFd, path: &Path, access: u64) -> io::Result<()> {
    let dir = open_path(path)?;
    let attr = PathBeneathAttr {
        allowed_access: access,
        parent_fd: dir.as_raw_fd(),
    };
    // SAFETY: add_rule copies attr; dir fd must stay open for the call.
    let result = unsafe {
        libc::syscall(
            SYS_LANDLOCK_ADD_RULE,
            ruleset.as_raw_fd(),
            LANDLOCK_RULE_PATH_BENEATH,
            &attr,
            0u32,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn open_path(path: &Path) -> io::Result<OwnedFd> {
    let c_path = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path contains NUL: {}", path.display()),
        )
    })?;
    // SAFETY: open with O_PATH|O_CLOEXEC for Landlock parent_fd.
    let fd = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_PATH | libc::O_CLOEXEC | libc::O_DIRECTORY,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fresh fd from open.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

fn restrict_self(ruleset: &OwnedFd) -> io::Result<()> {
    // SAFETY: applies the ruleset to the calling thread (the child before exec).
    let result = unsafe { libc::syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset.as_raw_fd(), 0u32) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
