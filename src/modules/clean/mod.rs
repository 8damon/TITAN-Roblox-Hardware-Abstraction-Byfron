pub mod delete;
pub mod kill;
pub mod referent;
pub mod shell;
pub mod startup;

use std::io;
use std::path::PathBuf;

use delete::*;
use referent::*;
use shell::*;
use startup::delete_roblox_startup_entry;
use tracing::{error, info};

use crate::modules::clean::kill::ArKillProcess;

pub struct TraceCleaner;

impl TraceCleaner {
    pub fn run(use_bootstrapper: bool, bootstrapper_name: Option<&str>) {
        info!("File system cleaning");

        if let Err(e) = clean_rbx(use_bootstrapper, bootstrapper_name) {
            error!("TraceCleaner::run() error: {}", e);
        }
    }
}

fn clean_rbx(use_bootstrapper: bool, bootstrapper_name: Option<&str>) -> io::Result<()> {
    if let Some(raw) = bootstrapper_name {
        let name = bootstrapper_process_name(raw);
        if !name.is_empty() {
            ArKillProcess(&name);
        }
    }

    ArKillProcess("RobloxPlayerBeta.exe");
    ArKillProcess("RobloxPlayerLauncher.exe");
    ArKillProcess("RobloxCrashHandler.exe");

    delete_roblox_startup_entry();

    let user_profile = get_user();
    let local_base = PathBuf::from(&user_profile).join("AppData/Local");

    let roblox_local = local_base.join("Roblox");

    if use_bootstrapper {
        if let Some(name) = bootstrapper_name {
            if name.trim().is_empty() {
                return Ok(());
            }

            let shortcut_name = format!("{name}.lnk");

            if let Some(target) = resolve_shortcut(&shortcut_name)
                && let Some(parent) = target.parent()
            {
                clean_versions(&parent.join("Versions"));
            }

            clean_versions(&local_base.join(name).join("Versions"));
        }
    } else {
        if let Some(rob_lnk) = resolve_shortcut("Roblox Player.lnk")
            && let (Some(_parent), Some(grandparent)) =
                (rob_lnk.parent(), rob_lnk.parent().and_then(|p| p.parent()))
        {
            clean_versions(grandparent);
        }

        clean_versions(&PathBuf::from(get_sys_drive()).join("Program Files (x86)/Roblox/Versions"));
    }

    //
    // AppData deletion
    //

    let dirs_to_delete = [
        "Temp/Roblox",
        "Roblox/logs",
        "Roblox/LocalStorage",
        "Roblox/Downloads",
        "Roblox/ClientSettings",
        "Roblox/rbx-storage",
        "Roblox/Versions",
    ];

    for sub in dirs_to_delete {
        let p = local_base.join(sub);
        remove_dir(&p);
    }

    let files_to_delete = [
        "rbx-storage.db",
        "rbx-storage.db-shm",
        "rbx-storage.db-wal",
        "rbx-storage.id",
        "frm.cfg",
    ];

    for file in files_to_delete {
        remove_file(&roblox_local.join(file));
    }

    //
    // XML referent mutation
    //

    mutate_referents(
        &roblox_local.join("GlobalBasicSettings_13.xml"),
        "UserGameSettings",
    )?;

    mutate_referents(
        &roblox_local.join("GlobalSettings_13.xml"),
        "UserGameSettings",
    )?;

    mutate_referents(
        &roblox_local.join("AnalysticsSettings.xml"),
        "GoogleAnalyticsConfiguration",
    )?;

    Ok(())
}

fn bootstrapper_process_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    std::path::Path::new(trimmed)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| trimmed.to_string())
}
