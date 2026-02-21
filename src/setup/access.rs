use windows::core::Result;

use windows::Win32::Security::{
    CheckTokenMembership, CreateWellKnownSid, PSID, SECURITY_MAX_SID_SIZE,
    WinBuiltinAdministratorsSid,
};

use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

use windows::core::PCWSTR;

pub fn ArAccessCheck() -> Result<()> {
    if !is_admin() {
        show_admin_required_message();
        std::process::exit(1);
    }

    Ok(())
}

fn is_admin() -> bool {
    unsafe {
        // Allocate SID buffer
        let mut sid_buffer = [0u8; SECURITY_MAX_SID_SIZE as usize];
        let mut sid_size = sid_buffer.len() as u32;

        // Create Administrators group SID
        if CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            None,
            Some(PSID(sid_buffer.as_mut_ptr() as *mut _)),
            &mut sid_size,
        )
        .is_err()
        {
            return false;
        }

        let mut is_member = false.into();

        // Passing None means current process token
        if CheckTokenMembership(
            None,
            PSID(sid_buffer.as_mut_ptr() as *mut _),
            &mut is_member,
        )
        .is_err()
        {
            return false;
        }

        is_member.as_bool()
    }
}

fn show_admin_required_message() {
    unsafe {
        let text = wide(
            "Administrator privileges are required.\n\nRight-click and select 'Run as administrator'.",
        );
        let title = wide("TRS Engine - Privilege Required");

        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
