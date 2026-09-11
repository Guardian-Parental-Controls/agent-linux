use std::time::Duration;

use guardian_agent::UpdateOffer;
use guardian_agent::update_verify;

const DEFAULT_GITHUB_REPO: &str = "Guardian-Parental-Controls/agent-linux";

async fn download_release_bytes(
    client: &reqwest::Client,
    url: &str,
    label: &str,
) -> Result<Vec<u8>, String> {
    println!("Downloading {label} from: {url}");
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("HTTP request failed for {label}: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Server returned error code {} for {label}: {url}",
            response.status()
        ));
    }
    response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read {label} download stream: {error}"))
        .map(|bytes| bytes.to_vec())
}

pub async fn apply_update(offer: UpdateOffer) -> Result<(), String> {
    let target_version = offer
        .target_version
        .as_deref()
        .ok_or_else(|| "Missing target version".to_string())?;
    println!("Initializing auto-update to version {target_version}...");

    if !update_verify::is_valid_release_version(target_version) {
        return Err(format!(
            "Refusing auto-update: invalid release version '{target_version}'"
        ));
    }

    let (download_url, checksum_url) = match (offer.download_url, offer.checksum_url) {
        (Some(download), Some(checksum)) => (download, checksum),
        _ => {
            let arch = std::env::consts::ARCH;
            let target = match arch {
                "x86_64" => "x86_64-unknown-linux-gnu",
                "aarch64" => "aarch64-unknown-linux-gnu",
                other => return Err(format!("Unsupported architecture for auto-update: {other}")),
            };
            if !update_verify::is_valid_github_repo(DEFAULT_GITHUB_REPO) {
                return Err("Refusing auto-update: invalid default GitHub repository".to_string());
            }
            let asset_name = format!("guardian-agent-{target}.tar.gz");
            let checksum_name = format!("{asset_name}.sha256");
            (
                format!(
                    "https://github.com/{DEFAULT_GITHUB_REPO}/releases/download/{target_version}/{asset_name}"
                ),
                format!(
                    "https://github.com/{DEFAULT_GITHUB_REPO}/releases/download/{target_version}/{checksum_name}"
                ),
            )
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| format!("Failed to build HTTP client: {error}"))?;

    let bytes = download_release_bytes(&client, &download_url, "release archive").await?;
    let checksum_bytes = download_release_bytes(&client, &checksum_url, "release checksum").await?;
    let checksum_text = String::from_utf8(checksum_bytes)
        .map_err(|error| format!("Release checksum is not valid UTF-8: {error}"))?;
    update_verify::verify_release_asset(&bytes, &checksum_text)?;
    println!(
        "Downloaded and verified {} bytes successfully. Extracting archive...",
        bytes.len()
    );

    let cursor = std::io::Cursor::new(bytes);
    let tar = flate2::read::GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(tar);
    let mut binary_bytes = None;
    let entries = archive
        .entries()
        .map_err(|error| format!("Failed to read archive entries: {error}"))?;
    for entry_result in entries {
        let mut entry =
            entry_result.map_err(|error| format!("Failed to parse archive entry: {error}"))?;
        let path = entry
            .path()
            .map_err(|error| format!("Failed to get entry path: {error}"))?
            .to_path_buf();
        if path.file_name().and_then(|name| name.to_str()) == Some("guardian-agent") {
            use std::io::Read;
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|error| format!("Failed to read binary from archive: {error}"))?;
            binary_bytes = Some(buf);
            break;
        }
    }

    let binary_bytes = binary_bytes
        .ok_or_else(|| "Archive did not contain 'guardian-agent' binary".to_string())?;
    println!(
        "Extracted new binary ({} bytes). Performing self-replace...",
        binary_bytes.len()
    );

    let current_bin = std::env::current_exe()
        .map_err(|error| format!("Failed to determine current executable path: {error}"))?;
    let bin_dir = current_bin
        .parent()
        .ok_or_else(|| "Failed to get current executable directory".to_string())?;
    let temp_bin = bin_dir.join("guardian-agent.tmp");
    std::fs::write(&temp_bin, &binary_bytes)
        .map_err(|error| format!("Failed to write temporary binary file: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_bin, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| format!("Failed to set permissions on temporary binary: {error}"))?;
    }
    std::fs::rename(&temp_bin, &current_bin).map_err(|error| {
        let _ = std::fs::remove_file(&temp_bin);
        format!("Failed to rename/replace active executable: {error}")
    })?;
    println!("Auto-update completed successfully! Active executable replaced.");
    Ok(())
}
