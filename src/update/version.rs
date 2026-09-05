//! Release-tag comparison. Deliberately hand-rolled instead of pulling in
//! `semver`: the only strings we ever compare are our own release tags,
//! `vMAJOR.MINOR.PATCH` with an optional pre-release suffix.

use std::cmp::Ordering;

/// A parsed tag: the three numbers, plus whether a `-pre` suffix followed.
struct Parts {
    nums: [u64; 3],
    pre_release: bool,
}

fn parse(version: &str) -> Option<Parts> {
    let version = version.trim();
    let version = version.strip_prefix('v').unwrap_or(version);
    // `1.2.3-rc1` and `1.2.3+build` both split at the first marker.
    let (core, suffix) = match version.find(['-', '+']) {
        Some(i) => (&version[..i], &version[i..]),
        None => (version, ""),
    };
    let mut nums = [0u64; 3];
    let mut seen = 0usize;
    for (i, segment) in core.split('.').enumerate() {
        if i >= 3 || segment.is_empty() {
            return None;
        }
        nums[i] = segment.parse().ok()?;
        seen = i + 1;
    }
    if seen != 3 {
        return None;
    }
    Some(Parts {
        nums,
        pre_release: suffix.starts_with('-'),
    })
}

/// True when `latest` is strictly newer than `current` and worth offering.
///
/// Anything unparseable yields `false`: missing an update is a small cost,
/// nagging about a version we did not understand is a support burden.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let (Some(current), Some(latest)) = (parse(current), parse(latest)) else {
        return false;
    };
    match latest.nums.cmp(&current.nums) {
        // Never push anyone onto a pre-release they did not ask for.
        Ordering::Greater => !latest.pre_release,
        Ordering::Less => false,
        // 0.3.0 supersedes 0.3.0-rc1.
        Ordering::Equal => current.pre_release && !latest.pre_release,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_release_is_newer() {
        assert!(is_newer("0.2.1", "0.3.0"));
        assert!(is_newer("v0.2.1", "v0.3.0"));
        assert!(is_newer("0.3.0", "0.3.1"));
        assert!(is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn the_same_or_an_older_release_is_not_newer() {
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.3.0", "0.2.1"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn a_pre_release_is_never_offered() {
        // Someone running 0.3.0 must not be nudged onto 0.4.0-rc1.
        assert!(!is_newer("0.3.0", "0.4.0-rc1"));
        assert!(!is_newer("0.3.0", "v1.0.0-beta.2"));
    }

    #[test]
    fn a_final_release_supersedes_its_own_pre_release() {
        assert!(is_newer("0.3.0-rc1", "0.3.0"));
    }

    #[test]
    fn anything_unparseable_is_silently_not_newer() {
        // Missing a version is far better than nagging about a bogus one.
        assert!(!is_newer("0.3.0", "not-a-version"));
        assert!(!is_newer("0.3.0", ""));
        assert!(!is_newer("", "0.3.0"));
        assert!(!is_newer("0.3", "0.4"));
        assert!(!is_newer("0.3.0", "0.3.0.1"));
        assert!(!is_newer("0.3.0", "0.3.x"));
    }
}
