//! Resolves the `matlabc` binary and runtime-archive locations. Resolution
//! order: `$MATLABC_PATH`, then a config file, then the verified default. Kept
//! injectable (env value passed in) so it's unit-testable without touching the
//! real environment.

use std::path::{Path, PathBuf};

/// Verified default install on this machine (see plan).
pub const DEFAULT_MATLABC: &str = "/home/leonardo/work/matlab_llvm/build/matlabc";

/// Resolved external-tool locations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub matlabc_path: PathBuf,
    pub runtime_archive: PathBuf,
}

impl Settings {
    /// Resolve from an explicit env value (`None` = unset) and an optional
    /// config-file override, falling back to the default.
    pub fn resolve(env_matlabc: Option<&str>, config_matlabc: Option<&str>) -> Settings {
        let matlabc = env_matlabc
            .filter(|s| !s.is_empty())
            .or(config_matlabc.filter(|s| !s.is_empty()))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_MATLABC));
        let runtime_archive = runtime_archive_for(&matlabc);
        Settings { matlabc_path: matlabc, runtime_archive }
    }

    /// Resolve using the real process environment (`$MATLABC_PATH`).
    pub fn from_env() -> Settings {
        Settings::resolve(std::env::var("MATLABC_PATH").ok().as_deref(), None)
    }
}

/// The runtime archive sits next to the `matlabc` binary.
pub fn runtime_archive_for(matlabc: &Path) -> PathBuf {
    matlabc
        .parent()
        .map(|dir| dir.join("libMatlabRuntime.a"))
        .unwrap_or_else(|| PathBuf::from("libMatlabRuntime.a"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_takes_priority() {
        let s = Settings::resolve(Some("/opt/matlabc"), Some("/cfg/matlabc"));
        assert_eq!(s.matlabc_path, PathBuf::from("/opt/matlabc"));
        assert_eq!(s.runtime_archive, PathBuf::from("/opt/libMatlabRuntime.a"));
    }

    #[test]
    fn config_used_when_env_absent() {
        let s = Settings::resolve(None, Some("/cfg/matlabc"));
        assert_eq!(s.matlabc_path, PathBuf::from("/cfg/matlabc"));
    }

    #[test]
    fn falls_back_to_default() {
        let s = Settings::resolve(None, None);
        assert_eq!(s.matlabc_path, PathBuf::from(DEFAULT_MATLABC));
    }

    #[test]
    fn empty_env_is_ignored() {
        let s = Settings::resolve(Some(""), Some("/cfg/matlabc"));
        assert_eq!(s.matlabc_path, PathBuf::from("/cfg/matlabc"));
    }

    #[test]
    fn runtime_archive_is_sibling() {
        assert_eq!(
            runtime_archive_for(Path::new("/a/b/matlabc")),
            PathBuf::from("/a/b/libMatlabRuntime.a")
        );
    }
}
