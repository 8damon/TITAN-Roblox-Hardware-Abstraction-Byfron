use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use rand::RngCore;

pub fn wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

pub fn to_upper(s: &str) -> String {
    s.to_uppercase()
}

pub fn gen_random_mac() -> String {
    let mut rng = rand::thread_rng();
    let mut mac = [0u8; 6];

    rng.fill_bytes(&mut mac);

    mac[0] &= 0xFE; // not multicast
    mac[0] |= 0x02; // locally administered

    format!(
        "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

pub fn bounce_adapter(friendly_name: &str) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::thread::sleep;
    use std::time::Duration;
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    let cmd_base = format!(
        "netsh interface set interface name=\"{}\" admin=",
        friendly_name
    );

    let off = Command::new("cmd")
        .arg("/C")
        .arg(format!("{}disable >nul 2>&1", cmd_base))
        .creation_flags(CREATE_NO_WINDOW.0)
        .status();

    sleep(Duration::from_secs(1));

    let on = Command::new("cmd")
        .arg("/C")
        .arg(format!("{}enable >nul 2>&1", cmd_base))
        .creation_flags(CREATE_NO_WINDOW.0)
        .status();

    off.as_ref().map(|s| s.success()).unwrap_or(false)
        && on.as_ref().map(|s| s.success()).unwrap_or(false)
}
