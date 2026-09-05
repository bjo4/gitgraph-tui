//! Downloading, verifying and swapping in a new binary.

use std::path::Path;

use sha2::{Digest, Sha256};

use crate::update::UpdateAction;
use crate::update::check::REPO;

/// The platform → release-asset table. Must stay in lockstep with the `case`
/// blocks in `install.sh` and the build matrix in `release.yml`; those four
/// targets are the only ones a release publishes.
pub fn target_triple(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        _ => None,
    }
}

pub fn asset_name(os: &str, arch: &str, tag: &str) -> Option<String> {
    Some(format!(
        "gitgraph-tui-{tag}-{}.tar.gz",
        target_triple(os, arch)?
    ))
}

pub fn asset_url(tag: &str, asset: &str) -> String {
    format!("https://github.com/{REPO}/releases/download/{tag}/{asset}")
}

/// Read the digest out of `sha256sum` output: `<64 hex chars>  <filename>`.
pub fn parse_sha256_file(text: &str) -> Option<String> {
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    let digest = line.split_whitespace().next()?;
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(digest.to_ascii_lowercase())
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            use std::fmt::Write;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// True when `exe` sits in cargo's bin directory. `cargo_home` and `home` are
/// passed in rather than read from the environment so this stays testable.
pub fn is_cargo_bin(exe: &Path, cargo_home: Option<&Path>, home: Option<&Path>) -> bool {
    let candidates = [
        cargo_home.map(|root| root.join("bin")),
        home.map(|root| root.join(".cargo/bin")),
    ];
    candidates
        .iter()
        .flatten()
        .any(|dir| exe.parent() == Some(dir.as_path()))
}

/// Probe writability by actually creating a file. Permission bits alone lie
/// on read-only mounts and under ACLs.
pub fn dir_is_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".gitgraph-tui-write-probe.{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Decide what the popup may offer, before it is ever shown. Order matters:
/// a platform with no asset can never self-update, whatever else is true.
pub fn decide_action(
    tag: &str,
    os: &str,
    arch: &str,
    exe: &Path,
    cargo_home: Option<&Path>,
    home: Option<&Path>,
    dir_writable: bool,
) -> UpdateAction {
    let cargo_command =
        format!("cargo install --git https://github.com/{REPO} --tag {tag} --locked");
    if asset_name(os, arch, tag).is_none() || is_cargo_bin(exe, cargo_home, home) {
        return UpdateAction::Manual {
            command: cargo_command,
        };
    }
    if !dir_writable {
        let dir = exe.parent().unwrap_or(exe).display();
        return UpdateAction::Manual {
            command: format!(
                "curl -fsSL https://raw.githubusercontent.com/{REPO}/main/install.sh | sh   \
                 # {dir} is not writable"
            ),
        };
    }
    UpdateAction::SelfUpdate
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn the_four_released_platforms_map_to_their_assets() {
        // This table must stay in lockstep with install.sh and release.yml.
        assert_eq!(
            asset_name("linux", "x86_64", "v0.3.0").as_deref(),
            Some("gitgraph-tui-v0.3.0-x86_64-unknown-linux-musl.tar.gz")
        );
        assert_eq!(
            asset_name("linux", "aarch64", "v0.3.0").as_deref(),
            Some("gitgraph-tui-v0.3.0-aarch64-unknown-linux-musl.tar.gz")
        );
        assert_eq!(
            asset_name("macos", "x86_64", "v0.3.0").as_deref(),
            Some("gitgraph-tui-v0.3.0-x86_64-apple-darwin.tar.gz")
        );
        assert_eq!(
            asset_name("macos", "aarch64", "v0.3.0").as_deref(),
            Some("gitgraph-tui-v0.3.0-aarch64-apple-darwin.tar.gz")
        );
    }

    #[test]
    fn platforms_without_a_prebuilt_asset_map_to_nothing() {
        assert_eq!(asset_name("windows", "x86_64", "v0.3.0"), None);
        assert_eq!(asset_name("linux", "riscv64", "v0.3.0"), None);
        assert_eq!(asset_name("freebsd", "x86_64", "v0.3.0"), None);
    }

    #[test]
    fn the_asset_url_points_at_the_release_download_path() {
        assert_eq!(
            asset_url("v0.3.0", "gitgraph-tui-v0.3.0-x86_64-apple-darwin.tar.gz"),
            "https://github.com/bjo4/gitgraph-tui/releases/download/v0.3.0/\
gitgraph-tui-v0.3.0-x86_64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn a_sha256sum_line_yields_its_digest() {
        let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let text = format!("{digest}  gitgraph-tui-v0.3.0-x86_64-apple-darwin.tar.gz\n");
        assert_eq!(parse_sha256_file(&text).as_deref(), Some(digest));
    }

    #[test]
    fn an_uppercase_digest_is_normalised() {
        let text = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  x.tar.gz";
        assert_eq!(
            parse_sha256_file(text).as_deref(),
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
        );
    }

    #[test]
    fn a_malformed_checksum_file_yields_nothing() {
        assert_eq!(parse_sha256_file(""), None);
        assert_eq!(parse_sha256_file("\n\n"), None);
        assert_eq!(parse_sha256_file("deadbeef  short.tar.gz"), None);
        assert_eq!(parse_sha256_file(&format!("{}  x", "z".repeat(64))), None);
    }

    #[test]
    fn sha256_matches_the_known_empty_string_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_zero_pads_bytes_below_0x10() {
        // The canonical NIST vector for "abc". Its digest contains 0x01, 0x03
        // and 0x00, so a formatter that drops the zero padding produces a
        // shorter, different string and this assertion fails.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_binary_inside_cargos_bin_directory_is_recognised() {
        let cargo_home = PathBuf::from("/opt/cargo");
        let home = PathBuf::from("/home/u");
        assert!(is_cargo_bin(
            Path::new("/opt/cargo/bin/gitgraph-tui"),
            Some(&cargo_home),
            Some(&home)
        ));
        assert!(is_cargo_bin(
            Path::new("/home/u/.cargo/bin/gitgraph-tui"),
            None,
            Some(&home)
        ));
        assert!(!is_cargo_bin(
            Path::new("/home/u/.local/bin/gitgraph-tui"),
            Some(&cargo_home),
            Some(&home)
        ));
        assert!(!is_cargo_bin(
            Path::new("/usr/local/bin/gitgraph-tui"),
            None,
            None
        ));
    }

    #[test]
    fn a_writable_directory_is_detected_and_a_missing_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        assert!(dir_is_writable(dir.path()));
        assert!(!dir_is_writable(&dir.path().join("does-not-exist")));
    }

    #[test]
    fn the_probe_file_never_survives_the_check() {
        let dir = tempfile::tempdir().unwrap();
        assert!(dir_is_writable(dir.path()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    fn action(os: &str, exe: &str, writable: bool) -> UpdateAction {
        decide_action(
            "v0.3.0",
            os,
            "x86_64",
            Path::new(exe),
            None,
            Some(Path::new("/home/u")),
            writable,
        )
    }

    #[test]
    fn a_supported_platform_in_a_writable_dir_can_self_update() {
        assert_eq!(
            action("linux", "/home/u/.local/bin/gitgraph-tui", true),
            UpdateAction::SelfUpdate
        );
    }

    #[test]
    fn an_unsupported_platform_is_offered_a_cargo_command() {
        let UpdateAction::Manual { command } = action("windows", "C:/bin/gitgraph-tui.exe", true)
        else {
            panic!("expected a manual action");
        };
        assert!(command.contains("cargo install"));
        assert!(command.contains("--tag v0.3.0"));
    }

    #[test]
    fn a_cargo_installed_binary_is_offered_a_cargo_command() {
        // Overwriting it would leave `cargo install --list` describing a file
        // that is no longer the one on disk.
        let UpdateAction::Manual { command } =
            action("linux", "/home/u/.cargo/bin/gitgraph-tui", true)
        else {
            panic!("expected a manual action");
        };
        assert!(command.contains("cargo install"));
    }

    #[test]
    fn a_cargo_binary_in_an_unwritable_directory_still_gets_the_cargo_command() {
        // Both the cargo-bin branch and the unwritable-dir branch match this
        // input, so it is the only shape that can pin their order.
        let UpdateAction::Manual { command } = decide_action(
            "v0.3.0",
            "linux",
            "x86_64",
            Path::new("/home/u/.cargo/bin/gitgraph-tui"),
            None,
            Some(Path::new("/home/u")),
            false,
        ) else {
            panic!("expected a manual action");
        };
        assert!(
            command.contains("cargo install"),
            "cargo detection must win over the unwritable-dir branch, got: {command}"
        );
        assert!(!command.contains("install.sh"));
    }

    #[test]
    fn an_unwritable_directory_is_offered_the_installer_not_a_doomed_button() {
        let UpdateAction::Manual { command } =
            action("linux", "/usr/local/bin/gitgraph-tui", false)
        else {
            panic!("expected a manual action");
        };
        assert!(command.contains("install.sh"));
        assert!(command.contains("/usr/local/bin"));
    }
}
