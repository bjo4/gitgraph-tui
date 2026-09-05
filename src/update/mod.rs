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
