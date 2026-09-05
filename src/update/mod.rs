//! Update check and self-update.
//!
//! Everything that touches the network or replaces the binary runs on a
//! background thread; the main loop only ever does a non-blocking `try_recv`.
//! Nothing here is wired into `App::new` — `main.rs` injects it — so the
//! integration tests, which build apps via `App::new_at`, can never reach the
//! network.

pub mod cache;
pub mod check;
pub mod install;
pub mod version;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{SystemTime, UNIX_EPOCH};

/// Which offer the popup may make. Decided when the `Available` state is
/// produced, never at the moment the user presses `y`, so the popup never
/// shows a button that is guaranteed to fail. See spec §5.1.1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateAction {
    /// A prebuilt asset exists and the binary's directory is writable.
    SelfUpdate,
    /// Everything else: hand over a command instead of a doomed button.
    Manual { command: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    /// The release page, shown in the popup.
    pub url: String,
    pub action: UpdateAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStep {
    Downloading,
    Verifying,
    Extracting,
    Replacing,
}

impl InstallStep {
    pub fn label(self) -> &'static str {
        match self {
            Self::Downloading => "downloading…",
            Self::Verifying => "verifying…",
            Self::Extracting => "extracting…",
            Self::Replacing => "installing…",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available(UpdateInfo),
    Installing(InstallStep),
    Done {
        latest: String,
    },
    /// Only ever an *install* failure. A failed check returns to `Idle` and
    /// leaves nothing on screen — the user did not ask to check for updates.
    Failed {
        message: String,
    },
}

/// What a background thread sends back to the main loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateMessage {
    /// `already_prompted` decides popup vs. help-line hint (spec §6.1).
    Available {
        info: UpdateInfo,
        already_prompted: bool,
    },
    Step(InstallStep),
    Done {
        latest: String,
    },
    Failed {
        message: String,
    },
}

/// Testable core of [`is_disabled`]: the environment is passed in.
fn is_disabled_with(flag: bool, opt_out: Option<&str>, ci: Option<&str>) -> bool {
    if flag {
        return true;
    }
    if let Some(value) = opt_out
        && !value.is_empty()
        && value != "0"
    {
        return true;
    }
    ci.is_some_and(|value| !value.is_empty())
}

/// True when the update check must not run at all.
pub fn is_disabled(flag: bool) -> bool {
    let opt_out = std::env::var("GITGRAPH_NO_UPDATE_CHECK").ok();
    let ci = std::env::var("CI").ok();
    is_disabled_with(flag, opt_out.as_deref(), ci.as_deref())
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Record that this tag's popup has been shown, so later launches only get
/// the help-line hint. Best-effort: a read-only cache dir just means the
/// popup appears again next time, which is harmless.
pub fn mark_prompted(tag: &str) {
    let Some(path) = cache::cache_file() else {
        return;
    };
    let mut cached = cache::load(&path);
    cached.prompted = Some(tag.to_string());
    cache::store(&path, &cached);
}

/// Start the background update check. Returns `None` when checking is
/// disabled or there is nowhere to cache — in both cases nothing at all
/// happens, and the app never learns an update exists.
///
/// Every failure inside the thread is silent by design: the user opened a git
/// log viewer and did not ask us to talk to GitHub.
pub fn spawn_check() -> Option<Receiver<UpdateMessage>> {
    let cache_path = cache::cache_file()?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let current = env!("CARGO_PKG_VERSION").to_string();
        let now = now_secs();
        let mut cached = cache::load(&cache_path);

        let latest = if cache::is_fresh(&cached, now) {
            cached.latest.clone().unwrap_or_default()
        } else {
            let Ok(tag) = check::fetch_latest_tag(&check::latest_url()) else {
                return; // Offline, rate-limited, whatever. Say nothing.
            };
            cached.last_check = now;
            cached.latest = Some(tag.clone());
            cache::store(&cache_path, &cached);
            tag
        };

        if !version::is_newer(&current, &latest) {
            return;
        }
        let Ok(exe) = std::env::current_exe().and_then(|p| p.canonicalize()) else {
            return;
        };
        let dir_writable = exe.parent().is_some_and(install::dir_is_writable);
        let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let action = install::decide_action(
            &latest,
            std::env::consts::OS,
            std::env::consts::ARCH,
            &exe,
            cargo_home.as_deref(),
            home.as_deref(),
            dir_writable,
        );
        let _ = tx.send(UpdateMessage::Available {
            info: UpdateInfo {
                current,
                url: check::release_page_url(&latest),
                latest: latest.clone(),
                action,
            },
            already_prompted: cached.prompted.as_deref() == Some(latest.as_str()),
        });
    });
    Some(rx)
}

/// Start the self-update. Unlike the check, failures here are shown: the user
/// pressed a button and is owed an answer.
pub fn spawn_install(tag: String) -> Receiver<UpdateMessage> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let exe = match std::env::current_exe().and_then(|p| p.canonicalize()) {
            Ok(exe) => exe,
            Err(e) => {
                let _ = tx.send(UpdateMessage::Failed {
                    message: format!("cannot locate this binary: {e}"),
                });
                return;
            }
        };
        let step_tx = tx.clone();
        let on_step = move |step| {
            let _ = step_tx.send(UpdateMessage::Step(step));
        };
        let message = match install::run(&tag, &exe, &on_step) {
            Ok(()) => UpdateMessage::Done { latest: tag },
            Err(e) => UpdateMessage::Failed {
                message: format!("{e:#}"),
            },
        };
        let _ = tx.send(message);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_explicit_flag_disables_the_check() {
        assert!(is_disabled_with(true, None, None));
    }

    #[test]
    fn the_opt_out_variable_disables_the_check() {
        assert!(is_disabled_with(false, Some("1"), None));
        assert!(is_disabled_with(false, Some("true"), None));
    }

    #[test]
    fn an_empty_or_zero_opt_out_does_not_disable_the_check() {
        // Someone doing `GITGRAPH_NO_UPDATE_CHECK=0` means "leave it on".
        assert!(!is_disabled_with(false, Some("0"), None));
        assert!(!is_disabled_with(false, Some(""), None));
    }

    #[test]
    fn ci_disables_the_check() {
        assert!(is_disabled_with(false, None, Some("true")));
    }

    #[test]
    fn nothing_set_leaves_the_check_enabled() {
        assert!(!is_disabled_with(false, None, None));
        assert!(!is_disabled_with(false, None, Some("")));
    }

    #[test]
    fn install_steps_have_user_facing_labels() {
        assert_eq!(InstallStep::Downloading.label(), "downloading…");
        assert_eq!(InstallStep::Replacing.label(), "installing…");
    }
}
