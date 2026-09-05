//! Downloading, verifying and swapping in a new binary.

use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use sha2::{Digest, Sha256};

use crate::update::InstallStep;
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

/// The only entry name we will ever extract. `release.yml` packages with
/// `tar -C dist/pkg -czf … gitgraph-tui LICENSE README.md`, so the entries
/// are bare names with no directory prefix.
///
/// If release packaging ever switches to archiving a directory, the entry
/// becomes `pkg/gitgraph-tui`, this guard rejects it, and self-update stops
/// working silently. That coupling is noted in `release.yml`.
pub const BINARY_NAME: &str = "gitgraph-tui";

/// Extract the binary from a release tarball into `dest`.
///
/// Entries are matched by exact name. Anything containing a path separator,
/// `..`, or an absolute path is skipped outright — a crafted archive must not
/// be able to write anywhere except the path we chose.
pub fn extract_binary(gz_bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    let decoder = flate2::read::GzDecoder::new(gz_bytes);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().context("reading the archive")? {
        let mut entry = entry.context("reading an archive entry")?;
        let path = entry.path().context("decoding an entry path")?;
        let Some(name) = path.to_str() else {
            continue;
        };
        // Belt and braces. The invariant that actually protects us is that
        // `dest` comes from the caller and an entry's own path is never used
        // to build a write path, so nothing can land where we did not choose.
        // These checks are defence in depth for a future refactor that reaches
        // for `unpack_in` or similar.
        //
        // Their test coverage deliberately overlaps: on Unix every sample that
        // trips the `..` or absolute-path clause also contains a separator, so
        // removing either of those two clauses alone will not fail a test. Do
        // not "simplify" them away on the strength of a green suite.
        if name.contains("..")
            || name.contains('/')
            || name.contains('\\')
            || Path::new(name).is_absolute()
        {
            continue;
        }
        if name != BINARY_NAME {
            continue;
        }
        let mut out =
            std::fs::File::create(dest).with_context(|| format!("creating {}", dest.display()))?;
        std::io::copy(&mut entry, &mut out).context("writing the extracted binary")?;
        return Ok(());
    }
    anyhow::bail!("the archive did not contain a `{BINARY_NAME}` entry")
}

/// Swap `new` into `target`.
///
/// `new` must already live in `target`'s directory: `rename` is atomic only
/// within one filesystem. Overwriting the running binary this way is safe on
/// Unix — the process keeps the old inode alive until it exits.
pub fn atomic_replace(new: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(new, target)
}

/// A release tarball is a couple of megabytes; this only exists so a
/// misbehaving server cannot make us read forever.
pub const MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;

/// Download a URL into memory. Not unit-tested: it is the second and last
/// network wrapper, and holds no logic of its own.
pub fn download(url: &str, max_bytes: u64) -> anyhow::Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(concat!("gitgraph-tui/", env!("CARGO_PKG_VERSION")))
        .timeout_connect(Some(Duration::from_secs(3)))
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    response
        .body_mut()
        .with_config()
        .limit(max_bytes)
        .read_to_vec()
        .with_context(|| format!("reading the body of {url}"))
}

pub fn stage_dir_name(pid: u32) -> String {
    format!(".gitgraph-tui-update.{pid}")
}

/// Remove staging directories and write probes left behind by an interrupted
/// update. Quitting mid-download is a normal thing for a user to do; this is
/// how it gets tidied, rather than by fighting the shutdown path.
pub fn clean_stale_stages(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with(".gitgraph-tui-update.") {
            let _ = std::fs::remove_dir_all(entry.path());
        } else if name.starts_with(".gitgraph-tui-write-probe.") {
            // `dir_is_writable` removes its own probe, but if that unlink ever
            // fails the file would sit in the user's bin directory for good.
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Run the freshly extracted binary once before trusting it. The checksum
/// already proved it is the file we published; this proves it can actually
/// execute here, which a checksum cannot.
pub fn verify_new_binary(path: &Path, tag: &str) -> anyhow::Result<()> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .with_context(|| format!("running {} --version", path.display()))?;
    if !output.status.success() {
        anyhow::bail!("the downloaded binary exited with {}", output.status);
    }
    let reported = String::from_utf8_lossy(&output.stdout);
    let expected = tag.trim_start_matches('v');
    if !reported.contains(expected) {
        anyhow::bail!(
            "the downloaded binary reports `{}`, not {tag}",
            reported.trim()
        );
    }
    Ok(())
}

/// The whole self-update, start to finish. Runs on a background thread; each
/// phase is reported through `on_step` so the popup can say where it is.
///
/// `exe` must be the canonicalised path of the running binary.
pub fn run(tag: &str, exe: &Path, on_step: &dyn Fn(InstallStep)) -> anyhow::Result<()> {
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot determine the install directory"))?;
    let asset = asset_name(std::env::consts::OS, std::env::consts::ARCH, tag)
        .ok_or_else(|| anyhow::anyhow!("no prebuilt binary for this platform"))?;

    clean_stale_stages(dir);
    // The staging directory lives beside the binary so the final rename stays
    // on one filesystem; across devices it would fail with EXDEV.
    let stage = dir.join(stage_dir_name(std::process::id()));
    std::fs::create_dir_all(&stage).with_context(|| format!("creating {}", stage.display()))?;
    let result = install_into(tag, &asset, &stage, exe, on_step);
    let _ = std::fs::remove_dir_all(&stage);
    result
}

fn install_into(
    tag: &str,
    asset: &str,
    stage: &Path,
    exe: &Path,
    on_step: &dyn Fn(InstallStep),
) -> anyhow::Result<()> {
    on_step(InstallStep::Downloading);
    let url = asset_url(tag, asset);
    let archive = download(&url, MAX_ASSET_BYTES)?;
    let sums = download(&format!("{url}.sha256"), 4096)?;

    on_step(InstallStep::Verifying);
    let expected = parse_sha256_file(&String::from_utf8_lossy(&sums))
        .ok_or_else(|| anyhow::anyhow!("the release is missing a usable .sha256 file"))?;
    let actual = sha256_hex(&archive);
    if actual != expected {
        // Never retry and never degrade: a mismatch means corrupted or
        // tampered, and both deserve the same hard stop.
        anyhow::bail!("checksum mismatch — aborting (expected {expected}, got {actual})");
    }

    on_step(InstallStep::Extracting);
    let staged = stage.join(BINARY_NAME);
    extract_binary(&archive, &staged)?;
    verify_new_binary(&staged, tag)?;

    on_step(InstallStep::Replacing);
    atomic_replace(&staged, exe).with_context(|| format!("replacing {}", exe.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    /// Build a tar.gz in memory. `tar::Header::set_path` refuses paths
    /// containing `..`, so the name is written straight into the raw header
    /// field — that is the only way to forge the malicious archive the
    /// traversal test needs.
    fn make_tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (name, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            let bytes = name.as_bytes();
            let old = header.as_old_mut();
            let n = bytes.len().min(old.name.len() - 1);
            old.name[..n].copy_from_slice(&bytes[..n]);
            header.set_cksum();
            builder.append(&header, *content).unwrap();
        }
        let tar_bytes = builder.into_inner().unwrap();
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(&tar_bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn the_binary_is_extracted_from_a_release_shaped_archive() {
        // Matches how release.yml packages: bare names, no directory prefix.
        let gz = make_tar_gz(&[
            ("LICENSE", b"MIT" as &[u8]),
            ("gitgraph-tui", b"\x7fELF-pretend"),
            ("README.md", b"# readme"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        extract_binary(&gz, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"\x7fELF-pretend");
    }

    #[test]
    fn an_archive_without_the_binary_is_an_error_not_a_silent_success() {
        let gz = make_tar_gz(&[("LICENSE", b"MIT" as &[u8])]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out");
        assert!(extract_binary(&gz, &dest).is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn a_path_traversal_entry_is_refused() {
        // The archive carries a `../../evil` entry alongside the real binary.
        // Nesting the destination two directories deep means a successful
        // escape would land at `outer/evil`, so the test can assert the
        // escape did not happen rather than merely that `dest` looks right.
        let outer = tempfile::tempdir().unwrap();
        let nested = outer.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        let escape_target = outer.path().join("evil");

        let gz = make_tar_gz(&[
            ("../../evil", b"pwned" as &[u8]),
            ("gitgraph-tui", b"legit"),
        ]);
        let dest = nested.join("out");
        extract_binary(&gz, &dest).unwrap();

        // Security property first: this is what the test is named after, and
        // ordering it ahead of the happy-path check means a regression that
        // escapes fails HERE, with a message that says so, rather than
        // tripping over a missing `dest` further down.
        assert!(
            !escape_target.exists(),
            "the `../../evil` entry escaped to {}",
            escape_target.display()
        );
        let written: Vec<_> = std::fs::read_dir(&nested)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name())
            .collect();
        assert_eq!(
            written.len(),
            1,
            "extraction wrote something other than the binary: {written:?}"
        );

        assert_eq!(std::fs::read(&dest).unwrap(), b"legit");
    }

    #[test]
    fn a_nested_or_absolute_entry_never_counts_as_the_binary() {
        for name in ["pkg/gitgraph-tui", "/gitgraph-tui", "./gitgraph-tui"] {
            let gz = make_tar_gz(&[(name, b"nope" as &[u8])]);
            let dir = tempfile::tempdir().unwrap();
            let dest = dir.path().join("out");
            assert!(
                extract_binary(&gz, &dest).is_err(),
                "{name} must not be accepted as the binary"
            );
        }
    }

    #[test]
    fn corrupt_gzip_input_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(extract_binary(b"not gzip at all", &dir.path().join("out")).is_err());
    }

    #[test]
    fn a_replaced_file_takes_the_new_content_and_is_executable() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("gitgraph-tui");
        let staged = dir.path().join("staged");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&staged, b"new").unwrap();
        atomic_replace(&staged, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!staged.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

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

    #[test]
    fn stage_directories_are_hidden_and_pid_scoped() {
        let name = stage_dir_name(4242);
        assert!(name.starts_with(".gitgraph-tui-update."));
        assert!(name.ends_with("4242"));
    }

    #[test]
    fn stale_stage_directories_are_swept_but_nothing_else_is() {
        // A user who quit mid-download leaves one of these behind.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".gitgraph-tui-update.111")).unwrap();
        std::fs::create_dir(dir.path().join(".gitgraph-tui-update.222")).unwrap();
        std::fs::write(dir.path().join("gitgraph-tui"), b"binary").unwrap();
        std::fs::create_dir(dir.path().join("unrelated")).unwrap();
        std::fs::write(dir.path().join(".gitgraph-tui-write-probe.333"), b"").unwrap();

        clean_stale_stages(dir.path());

        assert!(!dir.path().join(".gitgraph-tui-update.111").exists());
        assert!(!dir.path().join(".gitgraph-tui-update.222").exists());
        assert!(
            dir.path().join("gitgraph-tui").exists(),
            "the binary survives"
        );
        assert!(dir.path().join("unrelated").exists(), "other dirs survive");
        assert!(!dir.path().join(".gitgraph-tui-write-probe.333").exists());
    }

    #[test]
    fn sweeping_a_directory_with_no_stages_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        clean_stale_stages(dir.path());
        clean_stale_stages(&dir.path().join("missing"));
    }

    #[test]
    fn a_binary_that_reports_the_wrong_version_is_rejected() {
        // Stand in for the real binary with a shell script, so this stays a
        // pure filesystem test with no network and no release download.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = tempfile::tempdir().unwrap();
            let fake = dir.path().join("fake");
            std::fs::write(&fake, "#!/bin/sh\necho 'gitgraph-tui 0.2.1'\n").unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(verify_new_binary(&fake, "v0.3.0").is_err());
            assert!(verify_new_binary(&fake, "v0.2.1").is_ok());
        }
    }

    #[test]
    fn a_binary_that_cannot_run_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("not-there");
        assert!(verify_new_binary(&missing, "v0.3.0").is_err());
    }
}
