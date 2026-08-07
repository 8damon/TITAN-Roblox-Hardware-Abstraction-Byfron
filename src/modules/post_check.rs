use crate::modules::adapters::ArSnapshotMacTargets;
use std::collections::HashMap;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

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
    let hklm = winreg::RegKey::predef(HKEY_LOCAL_MACHINE);
    let path = "SOFTWARE\\Microsoft\\Cryptography";

    let key = hklm
        .open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
        .ok()?;

    let value: String = key.get_value("MachineGuid").ok()?;

    if value.is_empty() {
        None
    } else {
        Some(value.trim().to_string())
    }
}
