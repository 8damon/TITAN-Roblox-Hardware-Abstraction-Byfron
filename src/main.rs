#![windows_subsystem = "windows"]
#![allow(non_snake_case)]

pub mod components;
mod engine;
mod etw;
mod modules;
mod setup;

use crate::{
    components::tracing::ArTracing,
    modules::{
        PE::headers::{ArWipeHeaders, TRUE},
        clean::kill::ArKillProcess,
    },
    setup::{
        access::ArAccessCheck,
        setup::{ArConfig, ArRunSetup},
    },
};

use std::ffi::c_void;
use tracing::error;
use windows::Win32::Foundation::HINSTANCE;

#[unsafe(no_mangle)]
unsafe extern "system" fn TSRS_CALLBACK(_hinst: HINSTANCE, reason: u32, _reserved: *mut c_void) {
    if reason == 1 {
        ArKillProcess("RobloxPlayerBeta.exe");
    }
}

#[used]
#[cfg_attr(target_env = "msvc", unsafe(link_section = ".CRT$XLB"))]
static TLS_ENTRY: unsafe extern "system" fn(HINSTANCE, u32, *mut c_void) = TSRS_CALLBACK;

fn main() {
    components::tracing::ArSetConsoleTracingMuted(true);
    ArEnsureWorkingDirectoryAtExeDir();

    let _ = ArAccessCheck();

    let cfg = match ArRunSetup() {
        Ok(c) => c,
        Err(e) => {
            error!("Initialization failed: {e}");
            std::process::exit(1);
        }
    };

    if cfg.general.require_rerun_after_setup {
        eprintln!("Setup finished. Re-run this executable to continue.");
        std::process::exit(0);
    }

    ArRunConfiguredEngine(cfg);
}

fn ArEnsureWorkingDirectoryAtExeDir() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(dir) = exe.parent() else {
        return;
    };
    let _ = std::env::set_current_dir(dir);
}

pub(crate) fn ArRunConfiguredEngine(cfg: ArConfig) {
    components::tracing::ArSetConsoleTracingMuted(false);
    ArTracing();

    let _veh_guard = components::VEH::ArVehGuard::start();
    let _perf_monitor = components::performance::ArPerformanceMonitor::start();

    unsafe {
        let _ = ArWipeHeaders(TRUE);
    }

    if let Err(e) = engine::TrsEngine::new(cfg).run() {
        error!("Engine failure: {e}");
        std::process::exit(1);
    }
}
