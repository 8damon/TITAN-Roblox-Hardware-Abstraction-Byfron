use std::fs;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use tracing::{debug, error, info, trace, warn};

pub fn run() -> bool {
    info!("Using official Roblox web installer");

    match fetch_version() {
        Ok(version) => {
            info!(%version, "Fetched latest Roblox version");

            let url = format!(
                "https://setup.rbxcdn.com/{}-RobloxPlayerInstaller.exe",
                version
            );

            debug!(%url, "Constructed download URL");

            match download_installer(&url) {
                Ok(path) => {
                    info!(?path, "Installer downloaded successfully");

                    match Command::new(&path).spawn() {
                        Ok(child) => {
                            info!(pid = child.id(), ?path, "Installer launched");
                            schedule_installer_cleanup(child, path.clone());
                            true
                        }
                        Err(e) => {
                            error!(error = %e, "Failed to launch installer");
                            if let Err(clean_err) = fs::remove_file(&path) {
                                debug!(error = %clean_err, ?path, "Cleanup after failed launch could not remove installer");
                            } else {
                                info!(?path, "Removed installer after failed launch");
                            }
                            false
                        }
                    }
                }
                Err(e) => {
                    error!(error = %e, "Installer download failed");
                    false
                }
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to fetch version info");
            false
        }
    }
}

fn fetch_version() -> Result<String, Box<dyn std::error::Error>> {
    trace!("Requesting latest WindowsPlayer version metadata");

    let resp = ureq::get("https://clientsettingscdn.roblox.com/v2/client-version/WindowsPlayer")
        .header("User-Agent", "titan-rs-installer")
        .call()?;

    debug!(status = ?resp.status(), "Version metadata HTTP response received");

    let body = resp.into_body().read_to_string()?;

    trace!("Parsing JSON response");

    let v: serde_json::Value = serde_json::from_str(&body)?;

    let version = v
        .get("clientVersionUpload")
        .and_then(|x| x.as_str())
        .ok_or("missing clientVersionUpload")?;

    Ok(version.to_string())
}

fn download_installer(url: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    info!(%url, "Downloading Roblox installer");

    let response = ureq::get(url)
        .header("User-Agent", "titan-rs-installer")
        .call()?;

    if !response.status().is_success() {
        warn!(status = ?response.status(), "Installer download returned non-success status");
        return Err(format!("HTTP {}", response.status()).into());
    }

    let out_path = std::env::temp_dir().join("RobloxPlayerInstaller.exe");

    debug!(?out_path, "Writing installer to temp directory");

    let mut reader = response.into_body().into_reader();
    let mut file = File::create(&out_path)?;

    io::copy(&mut reader, &mut file)?;

    info!(?out_path, "Installer saved to disk");

    Ok(out_path)
}

fn schedule_installer_cleanup(mut child: std::process::Child, path: PathBuf) {
    info!(?path, "Scheduled temp installer cleanup after process exit");

    thread::spawn(move || {
        match child.wait() {
            Ok(status) => {
                info!(
                    ?status,
                    ?path,
                    "Installer process exited; starting temp cleanup"
                );
            }
            Err(e) => {
                warn!(error = %e, ?path, "Failed waiting for installer process; attempting cleanup anyway");
            }
        }

        // The installer may still hold the file briefly after process exit.
        for attempt in 1..=20 {
            match fs::remove_file(&path) {
                Ok(_) => {
                    info!(?path, attempt, "Removed temp installer");
                    return;
                }
                Err(e) => {
                    if attempt == 20 {
                        warn!(error = %e, ?path, "Failed to remove temp installer after retries");
                        return;
                    }
                    debug!(error = %e, ?path, attempt, "Temp installer still locked; retrying");
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    });
}
