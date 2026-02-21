use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::modules::clean::kill::ArKillProcess;
use tracing::{debug, error, info, trace, warn};

pub fn run(path: &str, cli_flag: Option<&str>) -> bool {
    info!(%path, "Launching bootstrapper");

    let mut cmd = Command::new(path);

    let final_flag = match cli_flag {
        Some(flag) if !flag.trim().is_empty() => flag.trim(),
        _ => "-player",
    };

    debug!(%final_flag, "Resolved bootstrapper CLI flag");

    for arg in final_flag.split_whitespace() {
        trace!(%arg, "Adding CLI argument");
        cmd.arg(arg);
    }

    match cmd.spawn() {
        Ok(child) => {
            info!(pid = child.id(), "Bootstrapper launched successfully");
            if monitor_bootstrapper_runtime(child, path) {
                true
            } else {
                relaunch_bootstrapper(path, final_flag)
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to launch bootstrapper");
            false
        }
    }
}

fn monitor_bootstrapper_runtime(mut child: Child, path: &str) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(10) {
        match child.try_wait() {
            Ok(Some(status)) => {
                info!(?status, "Bootstrapper exited before watchdog timeout");
                return true;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => {
                warn!(error = %e, "Failed to monitor bootstrapper process");
                return true;
            }
        }
    }

    warn!(
        pid = child.id(),
        path, "Bootstrapper exceeded watchdog timeout; treating as failure and recovering"
    );

    let _ = child.kill();
    let _ = child.wait();
    kill_spawned_roblox_processes();
    false
}

fn relaunch_bootstrapper(path: &str, flag: &str) -> bool {
    warn!(%path, %flag, "Re-launching bootstrapper after watchdog recovery");
    let mut retry_cmd = Command::new(path);
    for arg in flag.split_whitespace() {
        retry_cmd.arg(arg);
    }

    match retry_cmd.spawn() {
        Ok(child) => {
            info!(pid = child.id(), "Bootstrapper re-launched successfully");
            true
        }
        Err(e) => {
            error!(error = %e, "Bootstrapper re-launch failed");
            false
        }
    }
}

fn kill_spawned_roblox_processes() {
    ArKillProcess("RobloxPlayerBeta.exe");
    ArKillProcess("RobloxPlayerLauncher.exe");
    ArKillProcess("RobloxCrashHandler.exe");
    info!("Killed Roblox processes during bootstrapper recovery");
}
