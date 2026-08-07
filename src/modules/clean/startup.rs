use super::delete::remove_file;
use super::shell::get_user;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use windows::core::{PCWSTR, w};
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_EXPAND_SZ, REG_MULTI_SZ,
    REG_SZ,
};
use winreg::{HKEY, RegKey};

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
    let hklm = RegKey::predef(root);
    let subkey_string = match unsafe { subkey.to_string() } {
        Ok(s) => s,
        Err(_) => return,
    };

    let key = match hklm.open_subkey_with_flags(&subkey_string, KEY_READ | KEY_SET_VALUE) {
        Ok(key) => key,
        Err(_) => return,
    };

    let mut to_delete = Vec::new();

    for value_result in key.enum_values() {
        let value = match value_result {
            Ok(value) => value,
            Err(_) => continue,
        };

        let (name, value_data) = value;
        if value_mentions_roblox(value_data.vtype as u32, &value_data.bytes) {
            to_delete.push(name);
        }
    }

    for name in to_delete {
        if let Err(e) = key.delete_value(&name) {
            debug!(error = ?e, "Failed to remove startup Run entry");
        } else {
            info!(%name, "Removed Roblox startup Run entry");
        }
    }
}

fn value_mentions_roblox(value_type: u32, data: &[u8]) -> bool {
    let text = if value_type == REG_SZ as u32
        || value_type == REG_EXPAND_SZ as u32
        || value_type == REG_MULTI_SZ as u32
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
