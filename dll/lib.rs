#![allow(non_snake_case)]

#[path = "../src/components/mod.rs"]
pub mod components;
#[path = "../src/engine.rs"]
pub mod engine;
#[path = "../src/etw/mod.rs"]
pub mod etw;
#[path = "../src/modules/mod.rs"]
pub mod modules;
#[path = "../src/setup/mod.rs"]
pub mod setup;

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use modules::adapters::ArNetworkSnapshot;
use modules::install::{bootstrapper, web};
use modules::post_check::{
    ArCaptureSpoofState, ArVerifySpoofApplied as ArVerifySpoofAppliedInner, PostCheckReport,
    SpoofStateSnapshot,
};
use tracing::warn;

#[repr(C)]
pub struct ArNetworkSnapshotC {
    pub network_type: u32,
    pub band: u32,
    pub speed_mbps: u64,
    pub has_speed: i32,
}

#[repr(C)]
pub struct ArSpoofReportC {
    pub machine_guid_changed: i32,
    pub mac_values_changed: u32,
    pub mac_values_total: u32,
    pub passed: i32,
}

#[repr(C)]
pub struct ArCycleOptions {
    pub clean_and_reinstall: i32,
    pub use_bootstrapper: i32,
    pub prefer_bootstrapper_install: i32,
    pub open_roblox_after_spoof: i32,
    pub verify_network: i32,
    pub verify_post: i32,
    pub network_verify_wait_ms: u32,
    pub bootstrapper_path: *const c_char,
    pub bootstrapper_cli_flag: *const c_char,
}

pub type ArEtwProcessCallback = unsafe extern "C" fn(
    event_kind: i32, // 1=start, 2=stop
    pid: u32,
    exe_kind: i32, // 1=player,2=launcher,3=crashhandler
    path: *const c_char,
    user_data: *mut c_void,
);
pub type ArEtwStateCallback = unsafe extern "C" fn(
    state_kind: i32, // 1=wait, 2=ready
    user_data: *mut c_void,
);

#[derive(Clone, Copy, Default)]
struct EtwCallbackConfig {
    process_cb: Option<ArEtwProcessCallback>,
    state_cb: Option<ArEtwStateCallback>,
    user_data: usize,
}

struct EtwBridge {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

static ETW_CALLBACKS: LazyLock<Mutex<EtwCallbackConfig>> =
    LazyLock::new(|| Mutex::new(EtwCallbackConfig::default()));
static ETW_BRIDGES: LazyLock<Mutex<HashMap<u64, EtwBridge>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static ETW_BRIDGE_ID: AtomicU64 = AtomicU64::new(1);

static SPOOF_STATE_STORE: LazyLock<Mutex<HashMap<u64, SpoofStateSnapshot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SPOOF_STATE_ID: AtomicU64 = AtomicU64::new(1);

fn cstr_opt(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .trim()
        .to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn as_bool(v: i32) -> bool {
    v != 0
}

fn network_type_to_code(s: &str) -> u32 {
    match s {
        "Ethernet" => 1,
        "Wi-Fi" => 2,
        "Hotspot" => 3,
        _ => 0,
    }
}

fn code_to_network_type(code: u32) -> String {
    match code {
        1 => "Ethernet".to_string(),
        2 => "Wi-Fi".to_string(),
        3 => "Hotspot".to_string(),
        _ => "Unknown".to_string(),
    }
}

fn band_to_code(band: Option<&str>) -> u32 {
    match band {
        Some("2.4 GHz") => 24,
        Some("5 GHz") => 5,
        Some("6 GHz") => 6,
        Some("2.4/5 GHz") => 245,
        _ => 0,
    }
}

fn code_to_band(code: u32) -> Option<String> {
    match code {
        24 => Some("2.4 GHz".to_string()),
        5 => Some("5 GHz".to_string()),
        6 => Some("6 GHz".to_string()),
        245 => Some("2.4/5 GHz".to_string()),
        _ => None,
    }
}

fn snapshot_to_c(s: &ArNetworkSnapshot) -> ArNetworkSnapshotC {
    ArNetworkSnapshotC {
        network_type: network_type_to_code(&s.network_type),
        band: band_to_code(s.band.as_deref()),
        speed_mbps: s.speed_mbps.unwrap_or(0),
        has_speed: if s.speed_mbps.is_some() { 1 } else { 0 },
    }
}

fn snapshot_from_c(s: &ArNetworkSnapshotC) -> ArNetworkSnapshot {
    ArNetworkSnapshot {
        network_type: code_to_network_type(s.network_type),
        band: code_to_band(s.band),
        speed_mbps: if s.has_speed != 0 {
            Some(s.speed_mbps)
        } else {
            None
        },
    }
}

fn write_report(out: *mut ArSpoofReportC, report: &PostCheckReport) {
    if out.is_null() {
        return;
    }
    unsafe {
        *out = ArSpoofReportC {
            machine_guid_changed: if report.machine_guid_changed { 1 } else { 0 },
            mac_values_changed: report.mac_values_changed as u32,
            mac_values_total: report.mac_values_total as u32,
            passed: if report.passed() { 1 } else { 0 },
        };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArKillProcess(process_name: *const c_char) -> i32 {
    let Some(name) = cstr_opt(process_name) else {
        return 0;
    };
    if modules::clean::kill::ArKillProcess(&name) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArRunTraceCleaner(use_bootstrapper: i32, bootstrapper_path: *const c_char) {
    let path = cstr_opt(bootstrapper_path);
    modules::clean::TraceCleaner::run(as_bool(use_bootstrapper), path.as_deref());
}

#[unsafe(no_mangle)]
pub extern "C" fn ArSpoofWMI() -> i32 {
    if modules::WMI::ArSpoofWMI() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArSpoofRegistry() -> i32 {
    if modules::registry::ArSpoofRegistry() {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArSpoofMAC() {
    modules::adapters::ArSpoofMAC(true);
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ArCaptureActiveNetworkSnapshot(out: *mut ArNetworkSnapshotC) -> i32 {
    if out.is_null() {
        return 0;
    }
    let Some(snapshot) = modules::adapters::ArCaptureActiveNetworkSnapshot() else {
        return 0;
    };
    unsafe { *out = snapshot_to_c(&snapshot) };
    1
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ArVerifyNetworkPreserved(before: *const ArNetworkSnapshotC, wait_ms: u32) -> i32 {
    if before.is_null() {
        return 0;
    }
    let before_rust = snapshot_from_c(unsafe { &*before });
    let ok = modules::adapters::ArVerifyNetworkPreservedAfterMacSpoof(
        &before_rust,
        Duration::from_millis(wait_ms as u64),
    );
    if ok { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArCaptureSpoofStateHandle() -> u64 {
    let id = SPOOF_STATE_ID.fetch_add(1, Ordering::Relaxed);
    let mut store = SPOOF_STATE_STORE
        .lock()
        .expect("spoof state mutex poisoned");
    store.insert(id, ArCaptureSpoofState());
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn ArReleaseSpoofStateHandle(handle: u64) -> i32 {
    let mut store = SPOOF_STATE_STORE
        .lock()
        .expect("spoof state mutex poisoned");
    if store.remove(&handle).is_some() {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArVerifySpoofApplied(before_handle: u64, out_report: *mut ArSpoofReportC) -> i32 {
    let store = SPOOF_STATE_STORE
        .lock()
        .expect("spoof state mutex poisoned");
    let Some(before) = store.get(&before_handle) else {
        return 0;
    };
    let report = ArVerifySpoofAppliedInner(before);
    write_report(out_report, &report);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ArInstallBootstrapper(path: *const c_char, cli_flag: *const c_char) -> i32 {
    let Some(path) = cstr_opt(path) else {
        return 0;
    };
    let flag = cstr_opt(cli_flag);
    if bootstrapper::run(&path, flag.as_deref()) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ArInstallWeb() -> i32 {
    if web::run() { 1 } else { 0 }
}

fn run_spoof_pipeline_internal(
    verify_network: bool,
    wait_ms: u32,
    verify_post: bool,
    out_report: *mut ArSpoofReportC,
) -> bool {
    let before_spoof = if verify_post {
        Some(ArCaptureSpoofState())
    } else {
        None
    };
    let pre_network = if verify_network {
        modules::adapters::ArCaptureActiveNetworkSnapshot()
    } else {
        None
    };

    let _ = modules::WMI::ArSpoofWMI();
    let _ = modules::registry::ArSpoofRegistry();
    modules::adapters::ArSpoofMAC(true);

    if let Some(before) = pre_network.as_ref() {
        let _ = modules::adapters::ArVerifyNetworkPreservedAfterMacSpoof(
            before,
            Duration::from_millis(wait_ms as u64),
        );
    }

    if let Some(before) = before_spoof.as_ref() {
        let report = ArVerifySpoofAppliedInner(before);
        write_report(out_report, &report);
    }

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn ArRunSpoofPipeline(
    verify_network: i32,
    wait_ms: u32,
    verify_post: i32,
    out_report: *mut ArSpoofReportC,
) -> i32 {
    if run_spoof_pipeline_internal(
        as_bool(verify_network),
        wait_ms,
        as_bool(verify_post),
        out_report,
    ) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn ArRunCycle(opts: *const ArCycleOptions, out_report: *mut ArSpoofReportC) -> i32 {
    let defaults = ArCycleOptions {
        clean_and_reinstall: 1,
        use_bootstrapper: 0,
        prefer_bootstrapper_install: 0,
        open_roblox_after_spoof: 1,
        verify_network: 1,
        verify_post: 1,
        network_verify_wait_ms: 12_000,
        bootstrapper_path: std::ptr::null(),
        bootstrapper_cli_flag: std::ptr::null(),
    };
    let o = if opts.is_null() {
        &defaults
    } else {
        unsafe { &*opts }
    };

    if as_bool(o.clean_and_reinstall) {
        modules::clean::TraceCleaner::run(
            as_bool(o.use_bootstrapper),
            cstr_opt(o.bootstrapper_path).as_deref(),
        );
    }

    let _ = run_spoof_pipeline_internal(
        as_bool(o.verify_network),
        o.network_verify_wait_ms,
        as_bool(o.verify_post),
        out_report,
    );

    if as_bool(o.clean_and_reinstall) {
        if as_bool(o.use_bootstrapper) && as_bool(o.prefer_bootstrapper_install) {
            let Some(path) = cstr_opt(o.bootstrapper_path) else {
                return 1;
            };
            let flag = cstr_opt(o.bootstrapper_cli_flag);
            let _ = bootstrapper::run(&path, flag.as_deref());
        } else {
            let _ = web::run();
        }

        if !as_bool(o.open_roblox_after_spoof) {
            modules::clean::kill::ArKillProcess("RobloxPlayerBeta.exe");
            modules::clean::kill::ArKillProcess("RobloxPlayerLauncher.exe");
            modules::clean::kill::ArKillProcess("RobloxCrashHandler.exe");
        }
    }

    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ArSetEtwCallbacks(
    process_cb: Option<ArEtwProcessCallback>,
    state_cb: Option<ArEtwStateCallback>,
    user_data: *mut c_void,
) {
    let mut cfg = ETW_CALLBACKS.lock().expect("etw callback mutex poisoned");
    cfg.process_cb = process_cb;
    cfg.state_cb = state_cb;
    cfg.user_data = user_data as usize;
}

#[unsafe(no_mangle)]
pub extern "C" fn ArStartEtwCallbackBridge() -> u64 {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = Arc::clone(&stop);

    let worker = thread::spawn(move || {
        let Ok((_subsystem, rx)) = etw::ArStartETWSubsystem() else {
            return;
        };

        while !stop2.load(Ordering::Relaxed) {
            let alert = match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(a) => a,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            let cfg = *ETW_CALLBACKS.lock().expect("etw callback mutex poisoned");
            let user_data = cfg.user_data as *mut c_void;

            match alert {
                etw::RobloxAlert::ProcessStart { instance } => {
                    if let Some(cb) = cfg.process_cb {
                        let path = CString::new(instance.path.replace('\0', " ")).ok();
                        let path_ptr = path.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
                        unsafe { cb(1, instance.pid, exe_kind(instance.exe), path_ptr, user_data) };
                    }
                }
                etw::RobloxAlert::ProcessStop { instance } => {
                    if let Some(cb) = cfg.process_cb {
                        let path = CString::new(instance.path.replace('\0', " ")).ok();
                        let path_ptr = path.as_ref().map_or(std::ptr::null(), |s| s.as_ptr());
                        unsafe { cb(2, instance.pid, exe_kind(instance.exe), path_ptr, user_data) };
                    }
                }
                etw::RobloxAlert::SWait => {
                    if let Some(cb) = cfg.state_cb {
                        unsafe { cb(1, user_data) };
                    }
                }
                etw::RobloxAlert::SReady => {
                    if let Some(cb) = cfg.state_cb {
                        unsafe { cb(2, user_data) };
                    }
                }
            }
        }
    });

    let id = ETW_BRIDGE_ID.fetch_add(1, Ordering::Relaxed);
    let mut bridges = ETW_BRIDGES.lock().expect("etw bridge mutex poisoned");
    bridges.insert(
        id,
        EtwBridge {
            stop,
            worker: Some(worker),
        },
    );
    id
}

#[unsafe(no_mangle)]
pub extern "C" fn ArStopEtwCallbackBridge(handle: u64) -> i32 {
    let mut bridges = ETW_BRIDGES.lock().expect("etw bridge mutex poisoned");
    let Some(mut bridge) = bridges.remove(&handle) else {
        return 0;
    };
    bridge.stop.store(true, Ordering::Relaxed);
    if let Some(worker) = bridge.worker.take()
        && worker.join().is_err()
    {
        warn!("ETW bridge worker join failed");
    }
    1
}

fn exe_kind(exe: etw::RobloxExe) -> i32 {
    match exe {
        etw::RobloxExe::RobloxPlayerBeta => 1,
        etw::RobloxExe::RobloxPlayerLauncher => 2,
        etw::RobloxExe::RobloxCrashHandler => 3,
    }
}
