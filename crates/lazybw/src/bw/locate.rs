//! Locating the Bitwarden CLI, reading its version and deriving capabilities.

use semver::Version;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The minimum supported CLI version.
pub const MIN_VERSION: Version = Version::new(2025, 9, 0);

/// Version gates, per bw-integration.md §1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// The parsed version.
    pub version: Version,
    /// `bw list items --archived` (≥ 2025.10.0).
    pub archived_list: bool,
    /// Item types 6–8 may appear (≥ 2026.5.0).
    pub new_item_types: bool,
    /// `bw report password-health` (≥ 2026.8.0).
    pub password_health: bool,
}

impl Capabilities {
    /// Derives capabilities from a version.
    #[must_use]
    pub fn from_version(version: Version) -> Self {
        let gate = |major: u64, minor: u64| version >= Version::new(major, minor, 0);
        Self {
            archived_list: gate(2025, 10),
            new_item_types: gate(2026, 5),
            password_health: gate(2026, 8),
            version,
        }
    }

    /// True when the CLI is new enough to be used at all.
    #[must_use]
    pub fn supported(&self) -> bool {
        self.version >= MIN_VERSION
    }
}

/// A located CLI.
#[derive(Debug, Clone)]
pub struct BwLocation {
    /// Executable path.
    pub path: PathBuf,
    /// Version and capabilities.
    pub capabilities: Capabilities,
}

/// Why the CLI could not be located or used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocateError {
    /// Nothing was found on the configured path, `$LAZYBW_BW` or `$PATH`.
    NotFound {
        /// The configured path, when one was set.
        configured: Option<PathBuf>,
        /// The `$LAZYBW_BW` value, when set.
        env: Option<PathBuf>,
    },
    /// The executable could not be run.
    NotExecutable {
        /// The path that failed.
        path: PathBuf,
        /// The OS error.
        message: String,
    },
    /// The version could not be parsed.
    UnparseableVersion(String),
    /// The CLI is too old.
    TooOld(Version),
}

impl std::fmt::Display for LocateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { .. } => f.write_str("Bitwarden CLI not found"),
            Self::NotExecutable { path, message } => {
                write!(f, "Could not run {}: {message}", path.display())
            }
            Self::UnparseableVersion(version) => {
                write!(f, "Could not read the Bitwarden CLI version (\"{version}\")")
            }
            Self::TooOld(version) => write!(
                f,
                "Found bw {version}. lazybw needs 2025.9.0 or newer; update it with `npm update -g @bitwarden/cli`."
            ),
        }
    }
}

impl std::error::Error for LocateError {}

/// Parses `bw --version` output (`2026.10.1`, sometimes with a `v` prefix).
#[must_use]
pub fn parse_version(output: &str) -> Option<Version> {
    output
        .split_whitespace()
        .map(|token| token.trim_start_matches('v'))
        .find_map(|token| Version::parse(token).ok())
}

/// Locates the CLI using config → `$LAZYBW_BW` → `$PATH`.
pub fn locate(configured: &str) -> Result<BwLocation, LocateError> {
    let path = find_path(configured)?;
    version_of(&path).and_then(|version| {
        let capabilities = Capabilities::from_version(version.clone());
        if !capabilities.supported() {
            return Err(LocateError::TooOld(version));
        }
        Ok(BwLocation { path, capabilities })
    })
}

/// The cached result of a previous `bw --version` run.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct VersionCache {
    /// The executable the version belongs to.
    path: PathBuf,
    /// Its size at the time.
    size: u64,
    /// Its modification time in nanoseconds since the Unix epoch.
    mtime_ns: u128,
    /// The version string.
    version: String,
    /// When `bw --version` last ran, in seconds since the Unix epoch.
    #[serde(default)]
    checked_at: u64,
}

/// How long a cached version is trusted even when the executable looks
/// unchanged. Some installs (snap) put a launcher on `PATH` that stays the
/// same while the real CLI behind it is updated, so path, size and mtime
/// alone could keep an old version forever.
const VERSION_CACHE_MAX_AGE: u64 = 24 * 60 * 60;

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Locates the CLI like [`locate`], but reuses a cached version when the
/// executable has not changed.
///
/// `bw --version` starts Node and takes hundreds of milliseconds; that delay
/// used to sit in front of the first frame. The cache is keyed on the path,
/// size and modification time, so upgrading or replacing `bw` invalidates it.
pub fn locate_cached(
    configured: &str,
    cache_file: Option<&Path>,
) -> Result<BwLocation, LocateError> {
    let path = find_path(configured)?;
    let metadata = std::fs::metadata(&path).ok();
    if let (Some(cache_file), Some(metadata)) = (cache_file, &metadata)
        && let Ok(bytes) = std::fs::read(cache_file)
        && let Ok(cache) = serde_json::from_slice::<VersionCache>(&bytes)
        && cache.path == path
        && cache.size == metadata.len()
        && mtime_ns(metadata) == Some(cache.mtime_ns)
        && now_secs().saturating_sub(cache.checked_at) < VERSION_CACHE_MAX_AGE
        && let Ok(version) = Version::parse(&cache.version)
    {
        let capabilities = Capabilities::from_version(version);
        if capabilities.supported() {
            return Ok(BwLocation { path, capabilities });
        }
    }
    let version = version_of(&path)?;
    let capabilities = Capabilities::from_version(version.clone());
    if !capabilities.supported() {
        return Err(LocateError::TooOld(version));
    }
    if let (Some(cache_file), Some(metadata)) = (cache_file, &metadata) {
        let cache = VersionCache {
            path: path.clone(),
            size: metadata.len(),
            mtime_ns: mtime_ns(metadata).unwrap_or(0),
            version: version.to_string(),
            checked_at: now_secs(),
        };
        if let Some(parent) = cache_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let _ = crate::util::atomic::write_atomic(cache_file, &bytes, 0o600);
        }
    }
    Ok(BwLocation { path, capabilities })
}

/// Resolves the executable without running it.
fn find_path(configured: &str) -> Result<PathBuf, LocateError> {
    let configured =
        if configured.trim().is_empty() { None } else { Some(PathBuf::from(configured.trim())) };
    let env = std::env::var_os("LAZYBW_BW").filter(|v| !v.is_empty()).map(PathBuf::from);

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = &configured {
        candidates.push(path.clone());
    }
    if let Some(path) = &env {
        candidates.push(path.clone());
    }
    if let Ok(found) = which::which("bw") {
        candidates.push(found);
    }
    candidates.into_iter().find(|p| p.is_file()).ok_or(LocateError::NotFound { configured, env })
}

/// The modification time in nanoseconds, when the platform reports one.
fn mtime_ns(metadata: &std::fs::Metadata) -> Option<u128> {
    metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_nanos())
}

/// Reads the version of an executable.
pub fn version_of(path: &Path) -> Result<Version, LocateError> {
    let output =
        Command::new(path).arg("--version").env("NO_COLOR", "1").output().map_err(|err| {
            LocateError::NotExecutable { path: path.to_path_buf(), message: err.to_string() }
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_version(&stdout).or_else(|| parse_version(&stderr)).ok_or_else(|| {
        LocateError::UnparseableVersion(format!("{stdout}{stderr}").trim().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("2026.10.1\n"), Some(Version::new(2026, 10, 1)));
        assert_eq!(parse_version("v2025.9.0"), Some(Version::new(2025, 9, 0)));
        assert_eq!(parse_version("bw 2026.10.1"), Some(Version::new(2026, 10, 1)));
        assert_eq!(parse_version("nonsense"), None);
    }

    #[test]
    fn capability_gates() {
        let old = Capabilities::from_version(Version::new(2025, 9, 0));
        assert!(old.supported());
        assert!(!old.archived_list && !old.new_item_types && !old.password_health);

        let mid = Capabilities::from_version(Version::new(2026, 5, 2));
        assert!(mid.archived_list && mid.new_item_types && !mid.password_health);

        let new = Capabilities::from_version(Version::new(2026, 10, 1));
        assert!(new.archived_list && new.new_item_types && new.password_health);
    }

    #[test]
    fn too_old_is_reported() {
        let caps = Capabilities::from_version(Version::new(2025, 6, 0));
        assert!(!caps.supported());
        let err = LocateError::TooOld(caps.version);
        assert!(err.to_string().contains("2025.9.0"));
    }

    #[test]
    fn version_cache_avoids_rerunning_bw() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let script = dir.path().join("bw");
        std::fs::write(&script, "#!/bin/sh\necho run >> \"$0.runs\"\necho 2026.10.1\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cache = dir.path().join("bw-version.json");
        let configured = script.to_str().unwrap();

        let first = locate_cached(configured, Some(&cache)).unwrap();
        assert_eq!(first.capabilities.version, Version::new(2026, 10, 1));
        assert!(cache.exists(), "the version should be cached");

        let second = locate_cached(configured, Some(&cache)).unwrap();
        assert_eq!(second.capabilities, first.capabilities);

        let runs = std::fs::read_to_string(script.with_extension("runs")).unwrap();
        assert_eq!(runs.lines().count(), 1, "the cached run must not spawn bw again");

        // Replacing the executable invalidates the cache.
        std::fs::write(&script, "#!/bin/sh\necho run >> \"$0.runs\"\necho 2026.11.0\n").unwrap();
        let third = locate_cached(configured, Some(&cache)).unwrap();
        assert_eq!(third.capabilities.version, Version::new(2026, 11, 0));
        let runs = std::fs::read_to_string(script.with_extension("runs")).unwrap();
        assert_eq!(runs.lines().count(), 2);

        // A day-old entry is checked again even if the file looks the same.
        let mut stale: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&cache).unwrap()).unwrap();
        stale["checked_at"] = serde_json::json!(now_secs() - VERSION_CACHE_MAX_AGE - 1);
        std::fs::write(&cache, serde_json::to_vec(&stale).unwrap()).unwrap();
        locate_cached(configured, Some(&cache)).unwrap();
        let runs = std::fs::read_to_string(script.with_extension("runs")).unwrap();
        assert_eq!(runs.lines().count(), 3, "an expired cache entry must run bw again");
    }

    #[test]
    #[expect(unsafe_code, reason = "the test sets the environment it is reading")]
    fn missing_is_reported_with_sources() {
        unsafe {
            std::env::set_var("LAZYBW_BW", "/nonexistent/bw");
            std::env::set_var("PATH", "/nonexistent-path");
        }
        let err = locate("/nonexistent/configured-bw").unwrap_err();
        match err {
            LocateError::NotFound { configured, env } => {
                assert!(configured.is_some());
                assert!(env.is_some());
            }
            other => panic!("unexpected {other:?}"),
        }
        unsafe {
            std::env::remove_var("LAZYBW_BW");
            std::env::remove_var("PATH");
        }
    }
}
