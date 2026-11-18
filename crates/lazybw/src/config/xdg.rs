//! XDG base directories, read the same way on every platform.

use std::path::{Path, PathBuf};

/// `$var` when it holds an absolute path (the XDG spec ignores relative
/// ones), otherwise `fallback` under the home directory.
pub(super) fn dir(var: &str, home: Option<&Path>, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.map_or_else(|| PathBuf::from(fallback), |h| h.join(fallback)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_absolute_values_are_used() {
        let home = Path::new("/home/someone");
        #[expect(unsafe_code, reason = "a variable no other test reads")]
        unsafe {
            std::env::set_var("LAZYBW_TEST_XDG_ABS", "/srv/conf");
            std::env::set_var("LAZYBW_TEST_XDG_REL", "relative/conf");
        }
        assert_eq!(dir("LAZYBW_TEST_XDG_ABS", Some(home), ".config"), Path::new("/srv/conf"));
        assert_eq!(
            dir("LAZYBW_TEST_XDG_REL", Some(home), ".config"),
            Path::new("/home/someone/.config")
        );
        assert_eq!(
            dir("LAZYBW_TEST_XDG_UNSET", Some(home), ".local/state"),
            Path::new("/home/someone/.local/state")
        );
    }
}
