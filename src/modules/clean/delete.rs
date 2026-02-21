use std::fs;
use std::path::Path;
use tracing::{debug, info, trace, warn};

pub fn remove_file(path: &Path) {
    trace!(?path, "Attempting file removal");

    match fs::remove_file(path) {
        Ok(_) => info!(?path, "File removed"),
        Err(e) => debug!(?path, error = %e, "File removal skipped/failed"),
    }
}

pub fn remove_dir(path: &Path) {
    trace!(?path, "Attempting directory removal");

    match fs::remove_dir_all(path) {
        Ok(_) => info!(?path, "Directory removed"),
        Err(e) => debug!(?path, error = %e, "Directory removal skipped/failed"),
    }
}

pub fn bulk_delete(path: &Path, files: &[&str]) {
    debug!(?path, "Bulk delete starting");

    for f in files {
        let target = path.join(f);
        remove_file(&target);
    }
}

pub fn clean_versions(base_dir: &Path) {
    if !base_dir.exists() {
        trace!(?base_dir, "Base version directory does not exist");
        return;
    }

    info!(?base_dir, "Scanning version directories");

    if let Ok(iter) = fs::read_dir(base_dir) {
        for entry in iter.flatten() {
            if let Ok(ft) = entry.file_type()
                && ft.is_dir()
            {
                let name = entry.file_name().to_string_lossy().to_string();

                trace!(version = %name, "Found version directory");

                if name.starts_with("version-") {
                    info!(version = %name, "Cleaning version directory");

                    bulk_delete(
                        &entry.path(),
                        &[
                            "RobloxPlayerBeta.exe",
                            "RobloxPlayerBeta.dll",
                            "RobloxCrashHandler.exe",
                            "RobloxPlayerLauncher.exe",
                        ],
                    );
                }
            }
        }
    } else {
        warn!(?base_dir, "Failed to enumerate version directory");
    }
}
