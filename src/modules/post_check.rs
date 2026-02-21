use std::collections::HashMap;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, REG_EXPAND_SZ, REG_SZ, REG_VALUE_TYPE,
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
};
use windows::core::PCWSTR;

use crate::modules::adapters::ArSnapshotMacTargets;

pub struct SpoofStateSnapshot {
    machine_guid: Option<String>,
    mac_targets: HashMap<String, String>,
}

pub struct PostCheckReport {
    pub machine_guid_changed: bool,
    pub mac_values_changed: usize,
    pub mac_values_total: usize,
}

impl PostCheckReport {
    pub fn passed(&self) -> bool {
        self.machine_guid_changed || self.mac_values_changed > 0
    }
}

pub fn ArCaptureSpoofState() -> SpoofStateSnapshot {
    let mac_targets = ArSnapshotMacTargets()
        .into_iter()
        .collect::<HashMap<_, _>>();
    SpoofStateSnapshot {
        machine_guid: read_machine_guid(),
        mac_targets,
    }
}

pub fn ArVerifySpoofApplied(before: &SpoofStateSnapshot) -> PostCheckReport {
    let after = ArCaptureSpoofState();

    let machine_guid_changed = match (&before.machine_guid, &after.machine_guid) {
        (Some(prev), Some(now)) => prev != now,
        _ => false,
    };

    let mut mac_values_changed = 0usize;
    let mut mac_values_total = 0usize;

    for (guid, before_mac) in &before.mac_targets {
        mac_values_total += 1;
        if let Some(after_mac) = after.mac_targets.get(guid)
            && after_mac != before_mac
        {
            mac_values_changed += 1;
        }
    }

    PostCheckReport {
        machine_guid_changed,
        mac_values_changed,
        mac_values_total,
    }
}

fn read_machine_guid() -> Option<String> {
    let mut h_key = HKEY::default();
    let path = wide_null("SOFTWARE\\Microsoft\\Cryptography");

    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(path.as_ptr()),
            None,
            KEY_READ | KEY_WOW64_64KEY,
            &mut h_key,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }

    let value_name = wide_null("MachineGuid");
    let mut value_type = REG_VALUE_TYPE(0);
    let mut byte_len = 0u32;

    let rc_len = unsafe {
        RegQueryValueExW(
            h_key,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            None,
            Some(&mut byte_len),
        )
    };

    if rc_len != ERROR_SUCCESS
        || byte_len == 0
        || !(value_type == REG_SZ || value_type == REG_EXPAND_SZ)
    {
        unsafe {
            let _ = RegCloseKey(h_key);
        }
        return None;
    }

    let mut buf = vec![0u8; byte_len as usize];
    let rc_val = unsafe {
        RegQueryValueExW(
            h_key,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&mut value_type),
            Some(buf.as_mut_ptr()),
            Some(&mut byte_len),
        )
    };

    unsafe {
        let _ = RegCloseKey(h_key);
    }

    if rc_val != ERROR_SUCCESS || byte_len < 2 {
        return None;
    }

    let u16_len = (byte_len as usize) / 2;
    let wide = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, u16_len) };
    let nul = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    let value = String::from_utf16_lossy(&wide[..nul]).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
