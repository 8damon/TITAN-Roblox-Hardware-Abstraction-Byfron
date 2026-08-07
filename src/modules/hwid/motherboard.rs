use crate::components::generator::generate_random_serial;
use std::process::Command;
use tracing::{error, info};
use uuid::Uuid;
use winreg::RegKey;
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_WRITE};

#[derive(Debug)]
pub struct MotherboardSpoofing;

impl MotherboardSpoofing {
    pub fn spoof_system_uuid() -> Result<String, String> {
        let new_uuid = Uuid::new_v4().to_string().to_uppercase();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let profile_path = r"SYSTEM\CurrentControlSet\Control\IDConfigDB\Hardware Profiles\0001";

        match hklm.open_subkey_with_flags(profile_path, KEY_WRITE) {
            Ok(key) => {
                // Set HwProfileGuid
                key.set_value("HwProfileGuid", &format!("{{{}}}", new_uuid))
                    .map_err(|e| format!("Failed to set HwProfileGuid: {}", e))?;

                Ok(new_uuid)
            }
            Err(e) => Err(format!("Cannot access hardware profiles: {}", e)),
        }
    }

    pub fn spoof_baseboard_serial(original: &str) -> Result<String, String> {
        let spoofed = generate_random_serial(original);

        // Use AMIDEWIN for board serial
        let ps_cmd = format!(
            "powershell -Command \"Start-Process -FilePath 'C:\\\\Windows\\\\Fonts\\\\AMIDEWINx64.EXE' -ArgumentList '/SS {}' -Verb RunAs -Wait\"",
            spoofed
        );

        Command::new("cmd")
            .args(&["/C", &ps_cmd])
            .output()
            .map_err(|e| format!("Command failed: {}", e))?;

        Ok(spoofed)
    }

    pub fn get_current_baseboard_serial() -> Result<String, String> {
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                "Get-WmiObject -Class Win32_BaseBoard | Select-Object -ExpandProperty SerialNumber",
            ])
            .output()
            .map_err(|e| format!("WMI query failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err("Could not retrieve baseboard serial".to_string())
        }
    }
}

pub fn ArSpoofMotherboard() -> bool {
    info!("Starting motherboard spoofing");

    let mut overall_success = true;

    match MotherboardSpoofing::spoof_system_uuid() {
        Ok(uuid) => {
            info!("System UUID spoofed: {}", uuid);
        }
        Err(e) => {
            error!("UUID spoof failed: {}", e);
            overall_success = false;
        }
    }

    match MotherboardSpoofing::get_current_baseboard_serial()
        .and_then(|serial| MotherboardSpoofing::spoof_baseboard_serial(&serial))
    {
        Ok(serial) => {
            info!("Baseboard serial spoofed: {}", serial);
        }
        Err(e) => {
            error!("Baseboard spoof failed: {}", e);
            overall_success = false;
        }
    }

    info!("BIOS spoofing complete");
    overall_success
}
