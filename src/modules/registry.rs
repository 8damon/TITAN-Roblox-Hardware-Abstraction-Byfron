// net_registry

use std::borrow::Cow;
use std::ffi::OsString;

use crate::components::generator::gen_users;
use rand::{RngCore, thread_rng};
use regex::Regex;
use tracing::{error, info, warn};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, WIN32_ERROR};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetLogicalDriveStringsW, OPEN_EXISTING, ReadFile, SetFilePointer, WriteFile,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, FSCTL_UNLOCK_VOLUME,
};
use windows::core::PCWSTR;
use winreg::enums::{
    DELETE, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE,
    KEY_READ, KEY_SET_VALUE, KEY_WRITE, REG_BINARY,
};
use winreg::{RegKey, RegValue};

#[derive(Debug)]
pub struct HyperionTracking;

impl HyperionTracking {
    pub fn delete_systemreg_tracking() -> Result<bool, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let roblox_path = r"Software\Roblox";

        match hkcu.open_subkey_with_flags(roblox_path, KEY_WRITE) {
            Ok(roblox_key) => {
                // Delete the null-terminated SystemReg value
                match roblox_key.delete_value("\0SystemReg") {
                    Ok(_) => Ok(true),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                    Err(e) => Err(format!("Failed to delete SystemReg: {}", e)),
                }
            }
            Err(e) => Err(format!("Cannot access Roblox registry: {}", e)),
        }
    }
    pub fn clean_roblox_fingerprinting() -> Result<u32, String> {
        let mut deleted = 0;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let tracking_values = vec![
            (r"Software\Roblox", "ClientID"),
            (r"Software\Roblox", "DeviceId"),
            (r"Software\Roblox", "ServerTrackingId"),
        ];

        for (path, value) in tracking_values {
            if let Ok(key) = hkcu.open_subkey_with_flags(path, KEY_WRITE) {
                if key.delete_value(value).is_ok() {
                    deleted += 1;
                }
            }
        }

        Ok(deleted)
    }
}

pub fn ArSpoofRegistry() -> bool {
    info!("Starting registry spoofing");

    let mut overall_success = true;

    if let Err(e) = spoof_machine_guid() {
        error!("Failed to spoof MachineGUID | status={:?}", e);
        overall_success = false;
    }

    if let Err(e) = spoof_registered_user() {
        error!("Failed to spoof user info | status={:?}", e);
        overall_success = false;
    }

    if let Err(e) = spoof_logged_in_users() {
        error!("Failed to spoof logged in user info | status={:?}", e);
        overall_success = false;
    }

    let mut total_spoofed = 0;
    match EdidSpoofing::spoof_all_monitors(true) {
        Ok(count) => {
            if count > 0 {
                info!(
                    "Method 1: Spoofed {} EDID entry/entries (serial only)",
                    count
                );
                total_spoofed += count;
            } else {
                warn!("Method 1: No EDID entries were found or modified");
            }
        }
        Err(e) => {
            error!("Method 1: EDID spoofing failed | status={:?}", e);
            overall_success = false;
        }
    };

    match EdidSpoofing::spoof_all_monitors(false) {
        Ok(count) => {
            if count > 0 {
                info!("Method 2: Spoofed {} EDID entry/entries (full EDID)", count);
                total_spoofed += count;
            } else {
                warn!("Method 2: No EDID entries were found or modified");
            }
        }
        Err(e) => {
            error!("Method 2: EDID spoofing failed | status={:?}", e);
            overall_success = false;
        }
    };

    match EdidSpoofing::spoof_all_monitors_alternative() {
        Ok(count) => {
            if count > 0 {
                info!(
                    "Method 3: Spoofed {} EDID entry/entries (alternative method)",
                    count
                );
                total_spoofed += count;
            } else {
                warn!("Method 3: No EDID entries were found or modified");
            }
        }
        Err(e) => {
            error!("Method 3: EDID spoofing failed | status={:?}", e);
            overall_success = false;
        }
    };

    if total_spoofed > 0 {
        info!("Total EDID entries spoofed: {}", total_spoofed);
    } else if overall_success {
        warn!("No EDID entries were found in any method");
    } else {
        error!("All EDID spoofing methods failed");
    }

    match HyperionTracking::delete_systemreg_tracking() {
        Ok(true) => match HyperionTracking::clean_roblox_fingerprinting() {
            Ok(count) => {
                info!("Deleted {} Roblox tracking values", count);
            }
            Err(e) => {
                error!("Partial cleanup: {}", e);
                overall_success = false;
            }
        },
        Err(e) => {
            error!("Hyperion cleanup failed: {}", e);
            overall_success = false;
        }
        _ => {
            overall_success = false;
        }
    }

    info!(
        "Registry spoofing complete → {}",
        if overall_success {
            "OK"
        } else {
            "PARTIAL/FAILED"
        }
    );
    overall_success
}

fn spoof_machine_guid() -> Result<(), Box<dyn std::error::Error>> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Cryptography", KEY_SET_VALUE)?;

    let new_guid = uuid::Uuid::new_v4().to_string();
    key.set_value("MachineGuid", &new_guid)?;

    info!("MachineGUID spoofed | {}", new_guid);
    Ok(())
}

fn spoof_registered_user() -> Result<(), WIN32_ERROR> {
    let user = gen_users();
    let targets = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion",
            "RegisteredOwner",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion",
            "LastLoggedOnUser",
        ),
    ];

    for (root, path, value_name) in targets {
        let key = match RegKey::predef(root).open_subkey_with_flags(path, KEY_SET_VALUE) {
            Ok(k) => k,
            Err(e) => {
                warn!("Cannot open registry key | {} → {:?}", value_name, e);
                continue;
            }
        };

        if let Err(e) = key.set_value(value_name, &user) {
            warn!("Failed to set value | {} → {:?}", value_name, e);
            continue;
        }

        info!("User value spoofed | {} → {}", value_name, user);
    }

    Ok(())
}

pub struct EdidSpoofing;

impl EdidSpoofing {
    const EDID_SERIAL_OFFSET: usize = 8;
    const EDID_LENGTH: usize = 128;
    const EDID_CHECKSUM_OFFSET: usize = 127;

    pub fn randomize_edid_serial(edid: &mut [u8]) -> Result<(), String> {
        if edid.len() < Self::EDID_LENGTH {
            return Err("EDID buffer too small".to_string());
        }

        let random_serial: u32 = rand::random();
        edid[Self::EDID_SERIAL_OFFSET..Self::EDID_SERIAL_OFFSET + 4]
            .copy_from_slice(&random_serial.to_le_bytes());

        Self::recalculate_checksum(edid)?;

        Ok(())
    }

    pub fn generate_random_edid() -> [u8; 128] {
        let mut edid = [0u8; 128];
        thread_rng().fill_bytes(&mut edid);
        if let Err(e) = Self::recalculate_checksum(&mut edid) {
            warn!("Failed to recalculate checksum: {}", e);
        }
        edid
    }

    fn recalculate_checksum(edid: &mut [u8]) -> Result<(), String> {
        if edid.len() < Self::EDID_LENGTH {
            return Err("EDID buffer too small".to_string());
        }

        let sum: u32 = edid[..Self::EDID_CHECKSUM_OFFSET]
            .iter()
            .map(|&b| b as u32)
            .sum();

        let checksum = ((256 - (sum % 256)) % 256) as u8;
        edid[Self::EDID_CHECKSUM_OFFSET] = checksum;

        Ok(())
    }

    /*
    pub fn is_edid_valid(edid: &[u8]) -> bool {
        if edid.len() < Self::EDID_LENGTH {
            return false;
        }

        let sum: u32 = edid[..Self::EDID_CHECKSUM_OFFSET]
            .iter()
            .map(|&b| b as u32)
            .sum();

        (sum % 256) == 0
    }
    */

    pub fn spoof_all_monitors(randomize_serial: bool) -> Result<usize, String> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let display_path = r"SYSTEM\CurrentControlSet\Enum\DISPLAY";
        let mut modified_count = 0;

        match hklm.open_subkey(display_path) {
            Ok(display_key) => {
                for monitor in display_key.enum_keys().flatten() {
                    let monitor_path = format!("{}\\{}", display_path, monitor);

                    if let Ok(monitor_key) = hklm.open_subkey_with_flags(&monitor_path, KEY_READ) {
                        for device in monitor_key.enum_keys().flatten() {
                            let device_path = format!("{}\\{}", monitor_path, device);

                            if let Ok(device_key) =
                                hklm.open_subkey_with_flags(&device_path, KEY_WRITE)
                            {
                                if let Ok(edid_data) = device_key.get_raw_value("EDID") {
                                    let mut edid_bytes = edid_data.bytes.to_vec();

                                    let success = if randomize_serial {
                                        Self::randomize_edid_serial(&mut edid_bytes).is_ok()
                                    } else {
                                        let new_edid = Self::generate_random_edid();
                                        edid_bytes.copy_from_slice(&new_edid);
                                        true
                                    };

                                    if success {
                                        let edid_value = RegValue {
                                            bytes: Cow::Owned(edid_bytes),
                                            vtype: REG_BINARY,
                                        };

                                        if device_key.set_raw_value("EDID", &edid_value).is_ok() {
                                            modified_count += 1;
                                            info!("EDID spoofed for {}", device);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => return Err(format!("Cannot access display registry: {}", e)),
        }

        Ok(modified_count)
    }
    pub fn spoof_all_monitors_alternative() -> Result<usize, Box<dyn std::error::Error>> {
        let display_path = r"SYSTEM\CurrentControlSet\Enum\DISPLAY";
        let root = RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(display_path, KEY_READ | KEY_WRITE)?;

        let mut spoofed_count = 0;

        for vendor_result in root.enum_keys() {
            let vendor = vendor_result?;
            let device_base = format!(r"SYSTEM\CurrentControlSet\Enum\DISPLAY\{}", vendor);

            if let Ok(device_root) = RegKey::predef(HKEY_LOCAL_MACHINE)
                .open_subkey_with_flags(&device_base, KEY_READ | KEY_WRITE)
            {
                for instance_result in device_root.enum_keys() {
                    let instance = instance_result?;
                    let full_path = format!(r"{}\{}", device_base, instance);

                    let locations = [
                        format!(r"{}\Device Parameters", full_path),
                        format!(r"{}\Control\Device Parameters", full_path),
                        format!(r"{}\Monitor\Device Parameters", full_path),
                    ];

                    for loc in locations {
                        if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE)
                            .open_subkey_with_flags(&loc, KEY_READ | KEY_WRITE)
                        {
                            let edid = Self::generate_random_edid();
                            let edid_value = RegValue {
                                bytes: Cow::Borrowed(&edid),
                                vtype: REG_BINARY,
                            };

                            if key.set_raw_value("EDID", &edid_value).is_ok() {
                                info!("EDID spoofed | {} → {:?}", instance, &edid[8..12]);
                                spoofed_count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(spoofed_count)
    }
}

fn spoof_logged_in_users() -> Result<(), WIN32_ERROR> {
    let login_path = "Software\\Roblox\\RobloxStudio\\LoggedInUsersStore\\https:\\www.roblox.com";

    let key =
        match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(login_path, KEY_SET_VALUE) {
            Ok(k) => k,
            Err(e) => {
                warn!("Cannot open registry key | {:?} → {:?}", login_path, e);
                return Err(WIN32_ERROR(e.raw_os_error().unwrap_or(1) as u32));
            }
        };

    // Roblox Studio uses this value in registry as a placeholder for
    // non logged in clients:
    // {"":{"username":"","profilePicUrl":""}};
    let place_holder = "{\"\":{\"username\":\"\",\"profilePicUrl\":\"\"}}";
    let mut wide: OsString = OsString::from(place_holder);
    wide.push("\0"); // Add null terminator

    if let Err(e) = key.set_value("users", &wide) {
        warn!("Failed to set value | {:?} → {:?}", login_path, e);
        return Err(WIN32_ERROR(e.raw_os_error().unwrap_or(1) as u32));
    }

    info!(
        "Logged in users spoofed | {}",
        String::from_utf16_lossy(
            &login_path
                .encode_utf16()
                .chain(Some(0)) // Add null terminator
                .collect::<Vec<_>>()
        )
    );

    // delete tracking keys
    let keys_to_delete = [
        r"^\d+rbxRecentFiles_v03$",
        r"^rbxRecentRobloxApiGames_v02_\d+$",
        "RobloxStudioFirstTimeLoggedIn",
        "RobloxStudioLaunchTrackingGuid",
        "RobloxStudioMostRecentLogin",
    ];

    let re_files = Regex::new(keys_to_delete[0]).unwrap();
    let re_api_games = Regex::new(keys_to_delete[1]).unwrap();
    let base_path = "Software\\Roblox\\RobloxStudio";

    let delete_key = match RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
        base_path,
        KEY_QUERY_VALUE | KEY_ENUMERATE_SUB_KEYS | KEY_SET_VALUE | DELETE,
    ) {
        Ok(k) => k,
        Err(e) => {
            warn!(
                "Cannot reopen registry key for deletion | {:?} → {:?}",
                base_path, e
            );
            return Err(WIN32_ERROR(e.raw_os_error().unwrap_or(1) as u32));
        }
    };

    let mut to_delete = Vec::new();
    for subkey_name in delete_key.enum_keys().map(|x| x.unwrap()) {
        let should_delete = re_files.is_match(&subkey_name)
            || re_api_games.is_match(&subkey_name)
            || keys_to_delete[2..].contains(&subkey_name.as_str());

        if should_delete {
            to_delete.push(subkey_name);
        }
    }

    if to_delete.is_empty() {
        info!("No additional keys to delete found.");
        return Ok(());
    }

    for name in to_delete {
        match delete_key.delete_subkey(&name) {
            Ok(_) => info!("Deleted registry key: {}", name),
            Err(e) => warn!("Failed to delete key '{}': {:?}", name, e),
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub fn ArSpoofVolume() -> bool {
    info!("Starting volume serial spoofing");

    let mut buf = vec![0u16; 256];

    let len = unsafe { GetLogicalDriveStringsW(Some(&mut buf)) };
    if len == 0 || len > buf.len() as u32 {
        error!("GetLogicalDriveStringsW failed");
        return false;
    }

    let drives_str = String::from_utf16_lossy(&buf[0..len as usize]);
    let drives: Vec<&str> = drives_str.split('\0').filter(|s| !s.is_empty()).collect();

    let mut success = true;

    for drive in drives {
        let vol_path = format!("\\\\.\\{}", drive.trim_end_matches('\\'));
        let vol_wide: Vec<u16> = vol_path.encode_utf16().chain(std::iter::once(0)).collect();

        let h = match unsafe {
            CreateFileW(
                PCWSTR(vol_wide.as_ptr()),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        } {
            Ok(handle) => handle,
            Err(_) => {
                error!("Failed to open volume {}", drive);
                success = false;
                continue;
            }
        };

        let mut bytes: u32 = 0;
        let mut locked = false;

        if unsafe {
            DeviceIoControl(
                h,
                FSCTL_LOCK_VOLUME,
                None,
                0,
                None,
                0,
                Some(&mut bytes),
                None,
            )
            .is_ok()
        } {
            locked = true;
        }

        unsafe {
            let _ = DeviceIoControl(
                h,
                FSCTL_DISMOUNT_VOLUME,
                None,
                0,
                None,
                0,
                Some(&mut bytes),
                None,
            );
        }

        let mut sector = [0u8; 512];
        let mut read_bytes: u32 = 0;
        unsafe {
            let _ = ReadFile(h, Some(&mut sector), Some(&mut read_bytes), None);
        }

        let new_serial = thread_rng().next_u32();
        let mut spoofed = false;

        if sector[3..7] == [78, 84, 70, 83] {
            // NTFS
            sector[0x48..0x4c].copy_from_slice(&new_serial.to_le_bytes());
            spoofed = true;
        } else if sector[0x52..0x55] == [70, 65, 84] && sector[0x55] == 51 {
            // FAT32 'FAT3'
            sector[0x43..0x47].copy_from_slice(&new_serial.to_le_bytes());
            spoofed = true;
        } else if sector[0x36..0x3a] == [70, 65, 84, 32] {
            // FAT 'FAT '
            sector[0x27..0x2b].copy_from_slice(&new_serial.to_le_bytes());
            spoofed = true;
        }

        if spoofed {
            unsafe {
                SetFilePointer(h, 0, None, FILE_BEGIN);
            }
            let mut write_bytes: u32 = 0;
            unsafe {
                let _ = WriteFile(h, Some(&sector), Some(&mut write_bytes), None);
            }
            info!("Spoofed volume serial for {} to {:08X}", drive, new_serial);
        } else {
            warn!("Unsupported filesystem for {}", drive);
            success = false;
        }

        if locked {
            unsafe {
                let _ = DeviceIoControl(
                    h,
                    FSCTL_UNLOCK_VOLUME,
                    None,
                    0,
                    None,
                    0,
                    Some(&mut bytes),
                    None,
                );
            }
        }

        unsafe {
            let _ = CloseHandle(h);
        }
    }

    info!(
        "Volume spoofing finished → {}",
        if success { "success" } else { "partial/failed" }
    );
    success
}
