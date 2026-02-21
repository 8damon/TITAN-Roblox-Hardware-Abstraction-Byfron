use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
use windows::Win32::System::Registry::*;
use windows::core::{PCWSTR, PWSTR};

use super::util::{to_upper, wide_null};

const ADAPTER_CLASS_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4D36E972-E325-11CE-BFC1-08002BE10318}";

//
// Public API
//

pub fn find_adapter_registry_path(adapter_guid: &str) -> Option<String> {
    let mut h_base = HKEY::default();

    unsafe {
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_null(ADAPTER_CLASS_KEY).as_ptr()),
            None,
            KEY_READ | KEY_WOW64_64KEY,
            &mut h_base,
        ) != ERROR_SUCCESS
        {
            return None;
        }
    }

    let mut index = 0u32;

    loop {
        let mut name_buf = [0u16; 256];
        let mut name_len = name_buf.len() as u32;

        let rc = unsafe {
            RegEnumKeyExW(
                h_base,
                index,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        };

        index += 1;

        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }

        if rc != ERROR_SUCCESS {
            continue;
        }

        let subkey_name = utf16_trimmed(&name_buf, name_len);
        let sub_path = format!("{}\\{}", ADAPTER_CLASS_KEY, subkey_name);

        if adapter_matches(&sub_path, adapter_guid) {
            unsafe {
                let _ = RegCloseKey(h_base);
            }
            return Some(sub_path);
        }
    }

    unsafe {
        let _ = RegCloseKey(h_base);
    }
    None
}

pub fn set_network_address(reg_path: &str, mac: &str) -> bool {
    let mut h_key = HKEY::default();

    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_null(reg_path).as_ptr()),
            None,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            &mut h_key,
        )
    } != ERROR_SUCCESS
    {
        return false;
    }

    let mac_upper = to_upper(mac);
    let wide = wide_null(&mac_upper);

    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };

    let result = unsafe {
        RegSetValueExW(
            h_key,
            PCWSTR(wide_null("NetworkAddress").as_ptr()),
            None,
            REG_SZ,
            Some(bytes),
        )
    };

    unsafe {
        let _ = RegCloseKey(h_key);
    }

    result == ERROR_SUCCESS
}

pub fn get_network_address(reg_path: &str) -> Option<String> {
    let mut h_key = HKEY::default();

    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_null(reg_path).as_ptr()),
            None,
            KEY_READ | KEY_WOW64_64KEY,
            &mut h_key,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }

    let mut value_type = REG_VALUE_TYPE(0);
    let mut byte_len = 0u32;
    let query_len = unsafe {
        RegQueryValueExW(
            h_key,
            PCWSTR(wide_null("NetworkAddress").as_ptr()),
            None,
            Some(&mut value_type),
            None,
            Some(&mut byte_len),
        )
    };

    if query_len != ERROR_SUCCESS
        || byte_len == 0
        || !(value_type == REG_SZ || value_type == REG_EXPAND_SZ)
    {
        unsafe {
            let _ = RegCloseKey(h_key);
        }
        return None;
    }

    let mut buf = vec![0u8; byte_len as usize];
    let query_val = unsafe {
        RegQueryValueExW(
            h_key,
            PCWSTR(wide_null("NetworkAddress").as_ptr()),
            None,
            Some(&mut value_type),
            Some(buf.as_mut_ptr()),
            Some(&mut byte_len),
        )
    };

    unsafe {
        let _ = RegCloseKey(h_key);
    }

    if query_val != ERROR_SUCCESS || byte_len < 2 {
        return None;
    }

    let u16_len = (byte_len as usize) / 2;
    let wide = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u16, u16_len) };
    let value = utf16_trimmed(wide, u16_len as u32);
    if value.is_empty() { None } else { Some(value) }
}

//
// Internal Helpers
//

fn adapter_matches(reg_path: &str, target_guid: &str) -> bool {
    let mut h_sub = HKEY::default();

    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_null(reg_path).as_ptr()),
            None,
            KEY_READ | KEY_WOW64_64KEY,
            &mut h_sub,
        )
    } != ERROR_SUCCESS
    {
        return false;
    }

    let mut value_buf = [0u16; 256];
    let mut value_len = (value_buf.len() * 2) as u32;
    let mut value_type = REG_VALUE_TYPE(0);

    let result = unsafe {
        RegQueryValueExW(
            h_sub,
            PCWSTR(wide_null("NetCfgInstanceId").as_ptr()),
            None,
            Some(&mut value_type),
            Some(value_buf.as_mut_ptr() as *mut u8),
            Some(&mut value_len),
        )
    };

    let matched =
        if result == ERROR_SUCCESS && (value_type == REG_SZ || value_type == REG_EXPAND_SZ) {
            let guid = utf16_trimmed(&value_buf, value_len / 2);
            guid.eq_ignore_ascii_case(target_guid)
        } else {
            false
        };

    unsafe {
        let _ = RegCloseKey(h_sub);
    }

    matched
}

fn utf16_trimmed(buf: &[u16], len: u32) -> String {
    let len = len as usize;
    let slice = &buf[..len.min(buf.len())];

    let trimmed = slice
        .iter()
        .take_while(|&&c| c != 0)
        .copied()
        .collect::<Vec<u16>>();

    String::from_utf16_lossy(&trimmed)
}
