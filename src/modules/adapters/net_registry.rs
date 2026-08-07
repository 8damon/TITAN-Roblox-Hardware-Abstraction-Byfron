use super::util::to_upper;
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY};

const ADAPTER_CLASS_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4D36E972-E325-11CE-BFC1-08002BE10318}";

//
// Public API
//

pub fn find_adapter_registry_path(adapter_guid: &str) -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let h_base = match hklm.open_subkey_with_flags(ADAPTER_CLASS_KEY, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => key,
        Err(_) => return None,
    };

    for subkey_result in h_base.enum_keys() {
        let subkey_name = match subkey_result {
            Ok(name) => name,
            Err(_) => continue,
        };

        let sub_path = format!(r"{}\{}", ADAPTER_CLASS_KEY, subkey_name);

        if adapter_matches(&sub_path, adapter_guid) {
            return Some(sub_path);
        }
    }

    None
}

pub fn set_network_address(reg_path: &str, mac: &str) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    let key = match hklm.open_subkey_with_flags(reg_path, KEY_SET_VALUE | KEY_WOW64_64KEY) {
        Ok(key) => key,
        Err(_) => return false,
    };

    let mac_upper = to_upper(mac);

    // Set the NetworkAddress value
    match key.set_value("NetworkAddress", &mac_upper) {
        Ok(_) => true,
        Err(_) => false,
    }
}

pub fn get_network_address(reg_path: &str) -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey_with_flags(reg_path, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => key,
        Err(_) => return None,
    };

    match key.get_value::<String, _>("NetworkAddress") {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

//
// Internal Helpers
//

fn adapter_matches(reg_path: &str, target_guid: &str) -> bool {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey_with_flags(reg_path, KEY_READ | KEY_WOW64_64KEY) {
        Ok(key) => key,
        Err(_) => return false,
    };

    match key.get_value::<String, _>("NetCfgInstanceId") {
        Ok(guid) => guid.eq_ignore_ascii_case(target_guid),
        Err(_) => false,
    }
}
