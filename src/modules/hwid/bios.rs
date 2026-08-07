use crate::components::generator::generate_random_serial;
use std::process::Command;
use tracing::{error, info};
use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_WRITE};
use winreg::{HKEY, RegKey};

#[derive(Debug)]
pub struct BiosSpoofing;

impl BiosSpoofing {
    pub fn spoof_bios_serial(original_serial: &str) -> Result<String, String> {
        let hklm = RegKey::predef(HKEY::from(HKEY_LOCAL_MACHINE));
        let hardware_path = r"HARDWARE\DESCRIPTION\System";
        let spoofed = generate_random_serial(original_serial);

        match hklm.open_subkey_with_flags(hardware_path, KEY_WRITE) {
            Ok(_key) => {
                let ps_cmd = format!(
                    "powershell -Command \"Start-Process -FilePath 'C:\\\\Windows\\\\Fonts\\\\AMIDEWINx64.EXE' -ArgumentList '/BS {}' -Verb RunAs -Wait\"",
                    spoofed
                );

                match Command::new("cmd").args(&["/C", &ps_cmd]).output() {
                    Ok(output) => {
                        if output.status.success() {
                            Ok(spoofed)
                        } else {
                            Err("AMIDEWIN execution failed".to_string())
                        }
                    }
                    Err(e) => Err(format!("Command execution error: {}", e)),
                }
            }
            Err(e) => Err(format!("Registry access denied: {}", e)),
        }
    }

    /// WMI-based BIOS query fallback
    pub fn get_current_bios_serial() -> Result<String, String> {
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                "Get-WmiObject -Class Win32_BIOS | Select-Object -ExpandProperty SerialNumber",
            ])
            .output()
            .map_err(|e| format!("WMI query failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err("Could not retrieve BIOS serial".to_string())
        }
    }
}

pub fn ArSpoofBIOS() -> bool {
    info!("Starting BIOS spoofing");

    let mut success = true;

    match BiosSpoofing::get_current_bios_serial()
        .and_then(|serial| BiosSpoofing::spoof_bios_serial(&serial))
    {
        Ok(new_serial) => {
            info!("BIOS serial spoofed: {}", new_serial);
        }
        Err(e) => {
            error!("BIOS spoof failed: {}", e);
            success = false;
        }
    }

    info!("BIOS spoofing complete");
    success
}
