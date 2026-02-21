use std::env;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use tracing::{debug, info, trace, warn};

use windows::Win32::Foundation::MAX_PATH;
use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows::core::{GUID, Interface, PCWSTR};

use windows::Win32::System::Com::{
    self as Com, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, IPersistFile, STGM_READ,
};

use windows::Win32::UI::Shell::{
    FOLDERID_Profile, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};

pub fn get_user() -> String {
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Profile, KF_FLAG_DEFAULT, None).unwrap();

        let s = path.to_string().unwrap_or_default();

        CoTaskMemFree(Some(path.0 as _));

        trace!(profile_path = %s, "Resolved user profile");

        s
    }
}

pub fn get_sys_drive() -> String {
    let drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());

    trace!(drive = %drive, "Resolved system drive");
    drive
}

fn resolve_target(p: &Path) -> Option<String> {
    const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);

    trace!(?p, "Resolving shortcut target");

    unsafe {
        if CoInitializeEx(None, COINIT_MULTITHREADED).is_err() {
            warn!("COM initialization failed");
            return None;
        }

        let result = (|| {
            let link: IShellLinkW =
                CoCreateInstance(&CLSID_SHELL_LINK, None, Com::CLSCTX_ALL).ok()?;

            let persist: IPersistFile = link.cast().ok()?;

            let wide_path: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();

            persist.Load(PCWSTR(wide_path.as_ptr()), STGM_READ).ok()?;

            let mut buf = [0u16; MAX_PATH as usize];
            let mut find_data = WIN32_FIND_DATAW::default();

            link.GetPath(&mut buf, &mut find_data, 0).ok()?;

            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());

            let resolved = String::from_utf16_lossy(&buf[..len]);

            info!(target = %resolved, "Shortcut resolved");

            Some(resolved)
        })();

        CoUninitialize();

        result
    }
}

pub fn resolve_shortcut(name: &str) -> Option<PathBuf> {
    debug!(shortcut = %name, "Searching for shortcut");

    let shortcuts = vec![
        PathBuf::from(get_sys_drive())
            .join("ProgramData/Microsoft/Windows/Start Menu/Programs")
            .join(name),
        PathBuf::from(get_user())
            .join("AppData/Roaming/Microsoft/Windows/Start Menu/Programs")
            .join(name),
    ];

    for p in shortcuts {
        trace!(?p, "Checking shortcut path");

        if p.exists() {
            info!(?p, "Shortcut found");

            if let Some(target) = resolve_target(&p) {
                let target_path = PathBuf::from(target);

                if target_path.exists() {
                    return Some(target_path);
                }

                warn!(?target_path, "Resolved target does not exist");
            }
        }
    }

    warn!(shortcut = %name, "Shortcut not found");
    None
}
