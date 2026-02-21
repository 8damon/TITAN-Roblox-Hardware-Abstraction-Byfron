// registry.rs

use std::ptr;

use crate::components::generator::{gen_edid, gen_guid, gen_users};
use rand::{RngCore, thread_rng};
use tracing::{error, info, warn};
use windows::Win32::Foundation::{
    CloseHandle, ERROR_SUCCESS, GENERIC_READ, GENERIC_WRITE, WIN32_ERROR,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetLogicalDriveStringsW, OPEN_EXISTING, ReadFile, SetFilePointer, WriteFile,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, FSCTL_UNLOCK_VOLUME,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WRITE, REG_BINARY, REG_SAM_FLAGS,
    REG_SZ, REG_VALUE_TYPE, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryInfoKeyW,
    RegSetValueExW,
};
use windows::core::{PCWSTR, PWSTR, w};

struct RegKey(HKEY);

impl RegKey {
    fn open(path: PCWSTR, sam: REG_SAM_FLAGS) -> Result<Self, WIN32_ERROR> {
        let mut h = HKEY(ptr::null_mut());
        let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, path, None, sam, &mut h) };
        if status == ERROR_SUCCESS {
            Ok(RegKey(h))
        } else {
            Err(status)
        }
    }

    fn set_value(
        &self,
        name: PCWSTR,
        kind: REG_VALUE_TYPE,
        data: &[u8],
    ) -> Result<(), WIN32_ERROR> {
        let status = unsafe { RegSetValueExW(self.0, name, None, kind, Some(data)) };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(status)
        }
    }
}

impl Drop for RegKey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
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

    match spoof_edid() {
        Ok(count) => {
            if count > 0 {
                info!("Spoofed {} EDID entrie(s)", count);
            } else {
                warn!("No EDID entries were found or modified");
            }
        }
        Err(e) => {
            error!("EDID spoofing failed | status={:?}", e);
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

fn spoof_machine_guid() -> Result<(), WIN32_ERROR> {
    let path = w!("SOFTWARE\\Microsoft\\Cryptography");
    let key = RegKey::open(PCWSTR(path.as_ptr()), KEY_SET_VALUE)?;

    let new_guid = gen_guid();
    let wide = new_guid.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let bytes = unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };

    key.set_value(w!("MachineGuid"), REG_SZ, bytes)?;

    info!("MachineGUID spoofed | {}", new_guid);
    Ok(())
}

fn spoof_registered_user() -> Result<(), WIN32_ERROR> {
    let user = gen_users();
    let targets = [
        (
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion"),
            w!("RegisteredOwner"),
        ),
        (
            w!("SOFTWARE\\Microsoft\\Windows\\CurrentVersion"),
            w!("LastLoggedOnUser"),
        ),
    ];

    for (path, value_name) in targets {
        let key = match RegKey::open(PCWSTR(path.as_ptr()), KEY_SET_VALUE) {
            Ok(k) => k,
            Err(e) => {
                warn!("Cannot open registry key | {:?} → {:?}", value_name, e);
                continue;
            }
        };

        let wide = user.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let bytes =
            unsafe { std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2) };

        if let Err(e) = key.set_value(value_name, REG_SZ, bytes) {
            warn!("Failed to set value | {:?} → {:?}", value_name, e);
            continue;
        }

        info!("User value spoofed | {:?} → {}", value_name, user);
    }

    Ok(())
}

fn spoof_edid() -> Result<usize, WIN32_ERROR> {
    let display_path = w!("SYSTEM\\CurrentControlSet\\Enum\\DISPLAY");
    let root = RegKey::open(PCWSTR(display_path.as_ptr()), KEY_READ | KEY_WRITE)?;

    let mut subkey_count = 0u32;
    let mut max_subkey_len = 0u32;
    unsafe {
        let _ = RegQueryInfoKeyW(
            root.0,
            None,
            None,
            None,
            Some(&mut subkey_count),
            Some(&mut max_subkey_len),
            None,
            None,
            None,
            None,
            None,
            None,
        );
    }

    let mut spoofed_count = 0;

    for i in 0..subkey_count {
        let mut name_len = max_subkey_len + 1;
        let mut name_buf = vec![0u16; name_len as usize];

        if unsafe {
            RegEnumKeyExW(
                root.0,
                i,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            )
        } != ERROR_SUCCESS
        {
            continue;
        }

        let vendor = String::from_utf16_lossy(&name_buf[..name_len as usize]).to_string();

        let mut device_count = 0u32;
        let mut max_device_len = 0u32;

        let device_base = format!("{}\\{}", "SYSTEM\\CurrentControlSet\\Enum\\DISPLAY", vendor);
        let device_wide = device_base
            .encode_utf16()
            .chain(Some(0))
            .collect::<Vec<_>>();

        let device_root = match RegKey::open(PCWSTR(device_wide.as_ptr()), KEY_READ | KEY_WRITE) {
            Ok(k) => k,
            Err(_) => continue,
        };

        unsafe {
            let _ = RegQueryInfoKeyW(
                device_root.0,
                None,
                None,
                None,
                Some(&mut device_count),
                Some(&mut max_device_len),
                None,
                None,
                None,
                None,
                None,
                None,
            );
        }

        for j in 0..device_count {
            let mut len = max_device_len + 1;
            let mut buf = vec![0u16; len as usize];

            if unsafe {
                RegEnumKeyExW(
                    device_root.0,
                    j,
                    Some(PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            } != ERROR_SUCCESS
            {
                continue;
            }

            let instance = String::from_utf16_lossy(&buf[..len as usize]).to_string();

            let locations = [
                format!("{}\\{}", device_base, instance),
                format!("{}\\{}\\Device Parameters", device_base, instance),
                format!("{}\\{}\\Control\\Device Parameters", device_base, instance),
                format!("{}\\{}\\Monitor\\Device Parameters", device_base, instance),
            ];

            for loc in locations {
                let wide_loc = loc.encode_utf16().chain(Some(0)).collect::<Vec<_>>();

                let key = match RegKey::open(PCWSTR(wide_loc.as_ptr()), KEY_READ | KEY_WRITE) {
                    Ok(k) => k,
                    Err(_) => continue,
                };

                let mut edid = [0u8; 128];
                thread_rng().fill_bytes(&mut edid);

                if key.set_value(w!("EDID"), REG_BINARY, &edid).is_ok() {
                    let new_id = gen_edid();
                    info!("EDID spoofed | {} → {}", instance, new_id);
                    spoofed_count += 1;
                }
            }
        }
    }

    Ok(spoofed_count)
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
