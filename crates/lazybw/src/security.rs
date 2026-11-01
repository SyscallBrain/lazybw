//! Process hardening and secure temporary files.
//!
//! Called once at startup: make the process non-dumpable, disable core
//! dumps and set a restrictive umask so anything lazybw writes is private by
//! default. The secure temp directory lives in `$XDG_RUNTIME_DIR/lazybw`
//! (mode 0700) with `/dev/shm/lazybw-$UID` as the fallback.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Applies the process-wide hardening rules. Safe to call more than once.
pub fn harden_process() {
    // SAFETY: signal(2), prctl(PR_SET_DUMPABLE) and setrlimit are plain
    // syscalls with constant arguments; they are async-signal-safe and have no
    // memory safety implications.
    #[expect(unsafe_code, reason = "process hardening is libc calls with constant arguments")]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
        let limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        libc::setrlimit(libc::RLIMIT_CORE, &limit);
        libc::umask(0o077);
    }
}

/// Returns (creating it if needed) the private temp directory for lazybw.
///
/// `$XDG_RUNTIME_DIR/lazybw` when `$XDG_RUNTIME_DIR` is set and usable,
/// otherwise `/dev/shm/lazybw-<uid>`, otherwise `lazybw-<uid>` in the system
/// temp directory (`$TMPDIR`, which is where Termux on Android has one).
pub fn secure_temp_dir() -> io::Result<PathBuf> {
    let uid = {
        // SAFETY: getuid has no preconditions and cannot fail.
        #[expect(unsafe_code, reason = "getuid has no preconditions and cannot fail")]
        unsafe {
            libc::getuid()
        }
    };
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        let dir = PathBuf::from(runtime).join("lazybw");
        if ensure_private_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }
    let shm = PathBuf::from(format!("/dev/shm/lazybw-{uid}"));
    if ensure_private_dir(&shm).is_ok() {
        return Ok(shm);
    }
    let dir = std::env::temp_dir().join(format!("lazybw-{uid}"));
    ensure_private_dir(&dir)?;
    Ok(dir)
}

fn ensure_private_dir(dir: &Path) -> io::Result<()> {
    if !dir.exists() {
        fs::create_dir_all(dir)?;
    }
    let meta = fs::symlink_metadata(dir)?;
    if !meta.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a directory"));
    }
    // In a shared directory such as /tmp, another user could have created
    // the path first; only a directory we own is private.
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: getuid has no preconditions and cannot fail.
        #[expect(unsafe_code, reason = "getuid has no preconditions and cannot fail")]
        let uid = unsafe { libc::getuid() };
        if meta.uid() != uid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "the directory belongs to another user",
            ));
        }
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

/// Creates a new file with mode 0600 inside the private temp directory.
pub fn secure_temp_file(prefix: &str) -> io::Result<(PathBuf, File)> {
    let dir = secure_temp_dir()?;
    let unique = format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let path = dir.join(unique);
    let file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
    Ok((path, file))
}

/// Overwrites and removes a temporary file that held secrets.
pub fn shred_and_remove(path: &Path) -> io::Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && meta.is_file()
    {
        let len = meta.len();
        let mut file = OpenOptions::new().write(true).open(path)?;
        use std::io::{Seek, SeekFrom, Write};
        let zeros = vec![0u8; len.min(64 * 1024) as usize];
        let mut written = 0u64;
        file.seek(SeekFrom::Start(0))?;
        while written < len {
            let n = zeros.len().min((len - written) as usize);
            file.write_all(&zeros[..n])?;
            written += n as u64;
        }
        file.sync_all()?;
    }
    fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn temp_dir_is_0700() {
        let dir = secure_temp_dir().unwrap();
        let mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn temp_file_is_0600_and_removable() {
        let (path, file) = secure_temp_file("test").unwrap();
        drop(file);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        shred_and_remove(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn umask_is_restrictive_after_hardening() {
        harden_process();
        let (path, file) = secure_temp_file("umask").unwrap();
        drop(file);
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let _ = fs::remove_file(path);
    }
}
