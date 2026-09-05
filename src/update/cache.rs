//! The 24-hour update-check cache: a timestamp and two tags, three
//! `key=value` lines. No JSON, so no serde in the dependency tree.

use std::path::{Path, PathBuf};

pub const CACHE_TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cache {
    /// Unix seconds of the last completed check.
    pub last_check: i64,
    /// The newest tag GitHub reported.
    pub latest: Option<String>,
    /// The tag whose popup has already been shown once, so later launches
    /// only get the help-line hint.
    pub prompted: Option<String>,
}

/// `$XDG_CACHE_HOME/gitgraph-tui`, else the platform default.
///
/// `None` when `HOME` is unset. We then skip the update check entirely rather
/// than run an uncached check on every launch.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("gitgraph-tui"));
    }
    let home = std::env::var_os("HOME")?;
    if home.is_empty() {
        return None;
    }
    let home = PathBuf::from(home);
    if cfg!(target_os = "macos") {
        Some(home.join("Library/Caches/gitgraph-tui"))
    } else {
        Some(home.join(".cache/gitgraph-tui"))
    }
}

pub fn cache_file() -> Option<PathBuf> {
    Some(cache_dir()?.join("update.txt"))
}

/// Read the cache. A missing, unreadable, or malformed file is an empty
/// cache — never an error the user is shown.
pub fn load(path: &Path) -> Cache {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Cache::default();
    };
    let mut cache = Cache::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "last_check" => cache.last_check = value.parse().unwrap_or(0),
            "latest" if !value.is_empty() => cache.latest = Some(value.to_string()),
            "prompted" if !value.is_empty() => cache.prompted = Some(value.to_string()),
            _ => {}
        }
    }
    cache
}

/// Best-effort write. A read-only cache directory must never break the app,
/// so every failure here is swallowed.
pub fn store(path: &Path, cache: &Cache) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut text = format!("last_check={}\n", cache.last_check);
    if let Some(latest) = &cache.latest {
        text.push_str(&format!("latest={latest}\n"));
    }
    if let Some(prompted) = &cache.prompted {
        text.push_str(&format!("prompted={prompted}\n"));
    }
    let _ = std::fs::write(path, text);
}

/// True when the cached tag may be reused instead of hitting the network.
/// A `now` earlier than `last_check` (a skewed clock) counts as stale.
pub fn is_fresh(cache: &Cache, now: i64) -> bool {
    cache.latest.is_some() && (0..CACHE_TTL_SECS).contains(&(now - cache.last_check))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn a_stored_cache_reads_back_identically() {
        let dir = temp();
        let path = dir.path().join("update.txt");
        let written = Cache {
            last_check: 1_757_030_400,
            latest: Some("v0.3.0".to_string()),
            prompted: Some("v0.3.0".to_string()),
        };
        store(&path, &written);
        assert_eq!(load(&path), written);
    }

    #[test]
    fn store_creates_the_parent_directory() {
        let dir = temp();
        let path = dir.path().join("nested/deeper/update.txt");
        store(
            &path,
            &Cache {
                last_check: 1,
                latest: None,
                prompted: None,
            },
        );
        assert!(path.exists());
    }

    #[test]
    fn a_missing_file_is_an_empty_cache() {
        let dir = temp();
        assert_eq!(load(&dir.path().join("absent.txt")), Cache::default());
    }

    #[test]
    fn a_corrupt_file_is_an_empty_cache_not_an_error() {
        let dir = temp();
        let path = dir.path().join("update.txt");
        std::fs::write(&path, "\0\0garbage\nlast_check=not-a-number\nlatest=\n").unwrap();
        let cache = load(&path);
        assert_eq!(cache.last_check, 0);
        assert_eq!(cache.latest, None);
    }

    #[test]
    fn unknown_keys_are_ignored_so_a_future_field_cannot_break_an_old_binary() {
        let dir = temp();
        let path = dir.path().join("update.txt");
        std::fs::write(&path, "latest=v0.4.0\nchannel=beta\n").unwrap();
        assert_eq!(load(&path).latest, Some("v0.4.0".to_string()));
    }

    #[test]
    fn freshness_flips_exactly_at_the_ttl_boundary() {
        let cache = Cache {
            last_check: 1_000_000,
            latest: Some("v0.3.0".to_string()),
            prompted: None,
        };
        assert!(is_fresh(&cache, 1_000_000 + CACHE_TTL_SECS - 1));
        assert!(!is_fresh(&cache, 1_000_000 + CACHE_TTL_SECS));
    }

    #[test]
    fn a_cache_without_a_tag_is_never_fresh() {
        let cache = Cache {
            last_check: 1_000_000,
            latest: None,
            prompted: None,
        };
        assert!(!is_fresh(&cache, 1_000_001));
    }

    #[test]
    fn a_clock_that_went_backwards_is_treated_as_stale() {
        // A laptop resuming with a skewed clock must re-check, not sit on a
        // cache it thinks is from the future.
        let cache = Cache {
            last_check: 2_000_000,
            latest: Some("v0.3.0".to_string()),
            prompted: None,
        };
        assert!(!is_fresh(&cache, 1_000_000));
    }
}
