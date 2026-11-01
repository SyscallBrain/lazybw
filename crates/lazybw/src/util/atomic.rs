//! Atomic file writes with explicit permissions.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// Writes `contents` to `path` atomically: a sibling temporary file in the
/// destination directory, fsync, then rename. The file is created with
/// `mode` (use `0o600` for anything that may contain secrets).
///
/// The temporary name is unpredictable and the file is created exclusively
/// and without following links (`O_CREAT | O_EXCL | O_NOFOLLOW`): when the
/// destination is in a shared directory, nobody can plant a symlink under
/// the temporary name and have the contents written through it. A name that
/// already exists is skipped, not reused.
pub fn write_atomic(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("lazybw");
    let (tmp, mut file) = create_exclusive(dir, file_name, mode)?;
    let result = (|| {
        file.write_all(contents)?;
        // Permissions through the descriptor, never through the path.
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)?;
        if let Ok(dir_file) = File::open(dir) {
            let _ = dir_file.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Creates a new, uniquely named file next to the destination.
fn create_exclusive(dir: &Path, file_name: &str, mode: u32) -> io::Result<(PathBuf, File)> {
    let mut last_error = None;
    for _ in 0..16 {
        let tmp = dir.join(format!(".{file_name}.lazybw-tmp-{:016x}", random_u64()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&tmp)
        {
            Ok(file) => return Ok((tmp, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last_error = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("no free temporary name")))
}

/// 64 unpredictable bits from the kernel, with a time-based fallback (the
/// exclusive creation is what makes a guessed name harmless; randomness only
/// keeps a squatter from blocking every attempt).
fn random_u64() -> u64 {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    if File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).is_ok() {
        return u64::from_ne_bytes(bytes);
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default();
    nanos ^ (u64::from(std::process::id()) << 32)
}

use std::os::unix::fs::PermissionsExt;

#[cfg(test)]
mod tests {

    #[test]
    fn a_planted_symlink_is_never_written_through() {
        use std::os::unix::fs::symlink;
        let dir = TempDir::new().unwrap();
        let destination = dir.path().join("export.json");
        let victim = dir.path().join("victim");
        std::fs::write(&victim, "original").unwrap();
        // The old, predictable temporary name, planted as a symlink.
        let planted = dir.path().join(format!(".export.json.lazybw-tmp-{}", std::process::id()));
        symlink(&victim, &planted).unwrap();
        write_atomic(&destination, b"SYNTHETIC_SECRET", 0o600).unwrap();
        assert_eq!(std::fs::read(&victim).unwrap(), b"original");
        assert!(!destination.is_symlink());
        assert_eq!(std::fs::read(&destination).unwrap(), b"SYNTHETIC_SECRET");
        let mode = std::fs::metadata(&destination).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn concurrent_writers_do_not_share_a_temporary_file() {
        let dir = TempDir::new().unwrap();
        let destination = dir.path().join("state.json");
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let destination = destination.clone();
                std::thread::spawn(move || {
                    write_atomic(&destination, format!("writer {n}").as_bytes(), 0o600).unwrap();
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        let text = std::fs::read_to_string(&destination).unwrap();
        assert!(text.starts_with("writer "), "{text}");
        let leftovers = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(leftovers, 1, "temporary files left behind");
    }
    use super::*;
    use assert_fs::TempDir;

    #[test]
    fn writes_with_mode_and_replaces() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("export.json");
        write_atomic(&path, b"one", 0o600).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        write_atomic(&path, b"two", 0o600).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        let leftovers: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("lazybw-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }
}
