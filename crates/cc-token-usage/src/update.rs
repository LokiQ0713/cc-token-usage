use anyhow::{bail, Context, Result};
use ureq::ResponseExt;

const GITHUB_REPO: &str = "LokiQ0713/cc-token-usage";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub download_url: String,
}

/// Check for updates by following GitHub's release redirect (no API quota needed).
pub fn check_for_update() -> Result<UpdateStatus> {
    let target = target_triple();
    let asset_name = format!("cc-token-usage-{target}.tar.gz");

    // Follow redirect: github.com/REPO/releases/latest → github.com/REPO/releases/tag/vX.Y.Z
    // This uses GitHub CDN, not the REST API, so it's never rate-limited.
    let redirect_url = format!("https://github.com/{GITHUB_REPO}/releases/latest");
    let response = ureq::get(&redirect_url)
        .header("User-Agent", concat!("cc-token-usage/", env!("CARGO_PKG_VERSION")))
        .call()
        .context("failed to check latest release — check your internet connection")?;

    let final_url = response.get_uri().to_string();
    let tag_segment = final_url
        .rsplit('/')
        .next()
        .context("could not extract version from GitHub redirect")?;
    let latest = tag_segment.strip_prefix('v').unwrap_or(tag_segment);

    // Construct download URL directly (no API call needed)
    let download_url = format!(
        "https://github.com/{GITHUB_REPO}/releases/download/v{latest}/{asset_name}"
    );

    Ok(UpdateStatus {
        current_version: CURRENT_VERSION.to_string(),
        latest_version: latest.to_string(),
        update_available: version_gt(latest, CURRENT_VERSION),
        download_url,
    })
}

/// Download the latest release and replace the current binary.
pub fn perform_update() -> Result<()> {
    // Refuse to update if managed by npm
    if is_npm_managed() {
        bail!(
            "This binary is managed by npm.\n\
             Run `npm update -g cc-token-usage` to upgrade,\n\
             or use `npx cc-token-usage@latest` to always run the latest version."
        );
    }

    // Rustup-style permission probe: try creating a tempdir next to the binary.
    // If it fails, a package manager likely owns this location.
    check_write_permission()?;

    let status = check_for_update()?;

    if !status.update_available {
        eprintln!("Already up to date (v{})", status.current_version);
        return Ok(());
    }

    eprintln!(
        "Updating v{} → v{}",
        status.current_version, status.latest_version
    );
    eprintln!("Downloading...");

    // Download tar.gz
    let data = ureq::get(&status.download_url)
        .header("User-Agent", concat!("cc-token-usage/", env!("CARGO_PKG_VERSION")))
        .call()
        .context("failed to download release")?
        .into_body()
        .read_to_vec()
        .context("failed to read response body")?;

    // Extract binary from tar.gz
    let decoder = flate2::read::GzDecoder::new(&data[..]);
    let mut archive = tar::Archive::new(decoder);

    let current_exe = std::env::current_exe().context("cannot determine current executable path")?;
    let parent = current_exe
        .parent()
        .context("current executable has no parent directory")?;
    let tmp_path = parent.join(".cc-token-usage.new");

    let mut found = false;
    for entry in archive.entries().context("failed to read tar archive")? {
        let mut entry = entry.context("corrupt tar entry")?;
        let path = entry.path().context("invalid path in archive")?;
        if path.file_name().and_then(|n| n.to_str()) == Some("cc-token-usage") {
            entry
                .unpack(&tmp_path)
                .context("failed to extract binary")?;
            found = true;
            break;
        }
    }

    if !found {
        // Clean up temp file
        let _ = std::fs::remove_file(&tmp_path);
        bail!("archive does not contain cc-token-usage binary");
    }

    // Set executable permission
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o755))
            .context("failed to set executable permission")?;
    }

    // Atomic replacement: current → .old, .new → current
    let backup_path = parent.join(".cc-token-usage.old");
    let _ = std::fs::remove_file(&backup_path); // clean up any leftover

    std::fs::rename(&current_exe, &backup_path).context(
        "failed to replace binary — permission denied?\n\
         Try: sudo cc-token-usage update",
    )?;

    if let Err(e) = std::fs::rename(&tmp_path, &current_exe) {
        // Rollback: restore the old binary
        let _ = std::fs::rename(&backup_path, &current_exe);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e).context("failed to install new binary (rolled back)");
    }

    // Best-effort cleanup
    let _ = std::fs::remove_file(&backup_path);

    eprintln!("Updated to v{}", status.latest_version);
    Ok(())
}

/// Compile-time platform detection → target triple matching GitHub Release assets.
fn target_triple() -> &'static str {
    cfg_if_triple()
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn cfg_if_triple() -> &'static str {
    "aarch64-apple-darwin"
}
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
fn cfg_if_triple() -> &'static str {
    "x86_64-apple-darwin"
}
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn cfg_if_triple() -> &'static str {
    "x86_64-unknown-linux-musl"
}
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
fn cfg_if_triple() -> &'static str {
    "aarch64-unknown-linux-musl"
}
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
)))]
fn cfg_if_triple() -> &'static str {
    "unsupported"
}

/// Detect if the binary lives inside node_modules (npm-managed).
fn is_npm_managed() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.contains("node_modules")))
        .unwrap_or(false)
}

/// Rustup-style permission probe: try creating a temp file next to the binary.
/// Catches cases where a package manager installed to /usr/local/bin, /opt/homebrew/bin, etc.
fn check_write_permission() -> Result<()> {
    let exe = std::env::current_exe().context("cannot determine executable path")?;
    let dir = exe.parent().context("executable has no parent directory")?;

    match tempfile::tempfile_in(dir) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            bail!(
                "Cannot update: no write permission to {}.\n\
                 This binary may have been installed by a package manager.\n\
                 Try: sudo cc-token-usage update\n\
                 Or reinstall with: curl -fsSL https://raw.githubusercontent.com/LokiQ0713/cc-token-usage/master/install.sh | sh",
                dir.display()
            );
        }
        Err(e) => Err(e).context("permission check failed"),
    }
}

/// Simple semver comparison: returns true if `a` > `b`.
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.split('.').filter_map(|p| p.parse().ok());
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    };
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_gt() {
        assert!(version_gt("1.5.0", "1.4.0"));
        assert!(version_gt("2.0.0", "1.99.99"));
        assert!(version_gt("1.4.1", "1.4.0"));
        assert!(!version_gt("1.4.0", "1.4.0"));
        assert!(!version_gt("1.3.0", "1.4.0"));
    }

    #[test]
    fn test_target_triple_is_known() {
        assert_ne!(target_triple(), "unsupported");
    }

    #[test]
    fn test_npm_detection() {
        // Current test binary is not in node_modules
        assert!(!is_npm_managed());
    }
}
