use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
use windows::core::BOOL;

use crate::modules::clean::kill::ArKillProcess;

pub(crate) fn process_exists_by_names(names: &[&str]) -> bool {
    unsafe {
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut found = false;

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile)
                    .trim_matches(char::from(0))
                    .to_string();

                if names.iter().any(|n| name.eq_ignore_ascii_case(n)) {
                    found = true;
                    break;
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        found
    }
}

pub(crate) fn wait_for_process_to_close(name: &str, timeout: Duration) {
    let start = Instant::now();
    let mut seen = false;

    while start.elapsed() < timeout {
        if process_exists_by_names(&[name]) {
            seen = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    if !seen {
        return;
    }

    while process_exists_by_names(&[name]) {
        if start.elapsed() >= timeout {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

pub(crate) fn kill_roblox_processes() {
    ArKillProcess("RobloxPlayerBeta.exe");
    ArKillProcess("RobloxPlayerLauncher.exe");
    ArKillProcess("RobloxCrashHandler.exe");
}

pub(crate) fn any_roblox_window_visible() -> bool {
    struct WindowSearch {
        found: bool,
    }

    unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if unsafe { !IsWindowVisible(hwnd).as_bool() } {
            return BOOL(1);
        }

        if lparam.0 == 0 {
            return BOOL(1);
        }
        let search = unsafe { &mut *(lparam.0 as *mut WindowSearch) };

        let mut buf = [0u16; 128];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if len == 0 {
            return BOOL(1);
        }

        if title_is_roblox(&buf, len as usize) {
            search.found = true;
            return BOOL(0);
        }

        BOOL(1)
    }

    let mut search = WindowSearch { found: false };
    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_cb),
            LPARAM(&mut search as *mut _ as isize),
        );
    }
    search.found
}

pub(crate) fn pid_has_roblox_window(pid: u32) -> bool {
    struct WindowSearch {
        pid: u32,
        found: bool,
    }

    unsafe extern "system" fn enum_windows_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if unsafe { !IsWindowVisible(hwnd).as_bool() } {
            return BOOL(1);
        }

        if lparam.0 == 0 {
            return BOOL(1);
        }
        let search = unsafe { &mut *(lparam.0 as *mut WindowSearch) };

        let mut window_pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
        }
        if window_pid != search.pid {
            return BOOL(1);
        }

        let mut buf = [0u16; 128];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if len == 0 {
            return BOOL(1);
        }

        if title_is_roblox(&buf, len as usize) {
            search.found = true;
            return BOOL(0);
        }

        BOOL(1)
    }

    let mut search = WindowSearch { pid, found: false };
    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_cb),
            LPARAM(&mut search as *mut _ as isize),
        );
    }
    search.found
}

fn title_is_roblox(buf: &[u16], len: usize) -> bool {
    const ROBLOX: [u16; 6] = [82, 111, 98, 108, 111, 120];
    len == ROBLOX.len() && buf[..len] == ROBLOX
}
