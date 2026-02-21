use std::fs;
use std::path::PathBuf;

use tracing::{debug, info, warn};
use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_EXPAND_SZ,
    REG_MULTI_SZ, REG_SAM_FLAGS, REG_SZ, RegCloseKey, RegDeleteValueW, RegEnumValueW,
    RegOpenKeyExW,
};
use windows::core::{PCWSTR, PWSTR, w};

use super::delete::remove_file;
use super::shell::get_user;

const RUN_KEYS: &[(HKEY, PCWSTR)] = &[
    (
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ),
    (
        HKEY_CURRENT_USER,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
    ),
    (
        HKEY_LOCAL_MACHINE,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ),
    (
        HKEY_LOCAL_MACHINE,
        w!("Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
    ),
    (
        HKEY_LOCAL_MACHINE,
        w!("Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Run"),
    ),
    (
        HKEY_LOCAL_MACHINE,
        w!("Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\RunOnce"),
    ),
];

pub fn delete_roblox_startup_entry() {
    for (root, subkey) in RUN_KEYS {
        remove_matching_run_values(*root, *subkey);
    }

    remove_startup_shortcuts();
}

fn remove_matching_run_values(root: HKEY, subkey: PCWSTR) {
    let mut key = HKEY::default();
    let status = unsafe {
        RegOpenKeyExW(
            root,
            subkey,
            None,
            REG_SAM_FLAGS(KEY_READ.0 | KEY_SET_VALUE.0),
            &mut key,
        )
    };

    if status != ERROR_SUCCESS {
        return;
    }

    let mut to_delete: Vec<Vec<u16>> = Vec::new();
    let mut index = 0u32;

    loop {
        let mut name_buf = vec![0u16; 512];
        let mut name_len = name_buf.len() as u32;
        let mut value_type = 0u32;
        let mut data_buf = vec![0u8; 4096];
        let mut data_len = data_buf.len() as u32;

        let rc = unsafe {
            RegEnumValueW(
                key,
                index,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut value_type),
                Some(data_buf.as_mut_ptr()),
                Some(&mut data_len),
            )
        };

        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }

        if rc != ERROR_SUCCESS {
            index += 1;
            continue;
        }

        data_buf.truncate(data_len as usize);

        if value_mentions_roblox(value_type, &data_buf) {
            let name = &name_buf[..name_len as usize];
            let mut name_wide = name.to_vec();
            name_wide.push(0);
            to_delete.push(name_wide);
        }

        index += 1;
    }

    for name in to_delete {
        let rc = unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) };
        if rc == ERROR_SUCCESS {
            let value_name = String::from_utf16_lossy(&name[..name.len().saturating_sub(1)]);
            info!(%value_name, "Removed Roblox startup Run entry");
        } else {
            debug!(status = ?rc, "Failed to remove startup Run entry");
        }
    }

    unsafe {
        let _ = RegCloseKey(key);
    }
}

fn value_mentions_roblox(value_type: u32, data: &[u8]) -> bool {
    let text = if value_type == REG_SZ.0
        || value_type == REG_EXPAND_SZ.0
        || value_type == REG_MULTI_SZ.0
    {
        decode_reg_utf16(data)
    } else {
        String::from_utf8_lossy(data).to_string()
    };

    text.to_ascii_lowercase().contains("robloxplayerbeta.exe")
}

fn decode_reg_utf16(data: &[u8]) -> String {
    if data.len() < 2 {
        return String::new();
    }

    let mut u16s = Vec::with_capacity(data.len() / 2);
    let mut i = 0usize;
    while i + 1 < data.len() {
        u16s.push(u16::from_le_bytes([data[i], data[i + 1]]));
        i += 2;
    }

    let end = u16s.iter().position(|c| *c == 0).unwrap_or(u16s.len());
    String::from_utf16_lossy(&u16s[..end])
}

fn remove_startup_shortcuts() {
    let user_startup = PathBuf::from(get_user())
        .join("AppData")
        .join("Roaming")
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");

    let common_startup = PathBuf::from(r"C:\ProgramData")
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join("Startup");

    for dir in [user_startup, common_startup] {
        if !dir.exists() {
            continue;
        }

        let Ok(entries) = fs::read_dir(&dir) else {
            warn!(?dir, "Failed to enumerate startup directory");
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };

            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".lnk") && lower.contains("roblox") {
                remove_file(&path);
                info!(?path, "Removed Roblox startup shortcut");
            }
        }
    }
}
