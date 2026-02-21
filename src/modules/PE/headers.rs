#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

use windows::Win32::Foundation::{HANDLE, NTSTATUS};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::core::{Error, HRESULT, Result};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct UNICODE_STRING {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: *mut u16,
}

#[repr(C)]
pub struct RTL_USER_PROCESS_PARAMETERS {
    _Reserved1: [u8; 0x38],
    pub ImagePathName: UNICODE_STRING,
    pub CommandLine: UNICODE_STRING,
}

#[repr(C)]
pub struct PEB {
    _Reserved1: [u8; 0x20],
    pub ProcessParameters: *mut RTL_USER_PROCESS_PARAMETERS,
}

#[repr(C)]
struct PROCESS_BASIC_INFORMATION {
    ExitStatus: NTSTATUS,
    PebBaseAddress: *mut PEB,
    AffinityMask: usize,
    BasePriority: isize,
    UniqueProcessId: usize,
    InheritedFromUniqueProcessId: usize,
}

const PROCESS_BASIC_INFORMATION_CLASS: u32 = 0;

pub const TRUE: bool = true;

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQueryInformationProcess(
        ProcessHandle: HANDLE,
        ProcessInformationClass: u32,
        ProcessInformation: *mut std::ffi::c_void,
        ProcessInformationLength: u32,
        ReturnLength: *mut u32,
    ) -> NTSTATUS;
}

/// Wipes ImagePathName from PEB.  
/// If `wipe_cmdline` is true -> also wipes CommandLine.
///
/// # Safety
/// This mutates live process memory via raw pointers into the current process PEB.
/// Callers must ensure the process parameters pointers are valid for writes for the
/// entire duration of this call.
pub unsafe fn ArWipeHeaders(wipe_cmdline: bool) -> Result<()> {
    let mut pbi = PROCESS_BASIC_INFORMATION {
        ExitStatus: NTSTATUS(0),
        PebBaseAddress: std::ptr::null_mut(),
        AffinityMask: 0,
        BasePriority: 0,
        UniqueProcessId: 0,
        InheritedFromUniqueProcessId: 0,
    };

    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            PROCESS_BASIC_INFORMATION_CLASS,
            &mut pbi as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of_val(&pbi) as u32,
            std::ptr::null_mut(),
        )
    };

    if status.0 != 0 {
        return Err(Error::from_hresult(HRESULT(status.0)));
    }

    let peb = unsafe { &mut *pbi.PebBaseAddress };
    let params_ptr = peb.ProcessParameters;
    if params_ptr.is_null() {
        return Err(Error::from(HRESULT(0x80004005u32 as i32)));
    }

    let params = unsafe { &mut *params_ptr };

    if !params.ImagePathName.Buffer.is_null() && params.ImagePathName.Length > 0 {
        let byte_len = params.ImagePathName.Length as usize;
        unsafe {
            std::ptr::write_bytes(
                params.ImagePathName.Buffer as *mut u8,
                0u8,
                byte_len + std::mem::size_of::<u16>(),
            );
        }
        params.ImagePathName.Length = 0;
        params.ImagePathName.MaximumLength = 0;
    }

    if wipe_cmdline && !params.CommandLine.Buffer.is_null() && params.CommandLine.Length > 0 {
        let byte_len = params.CommandLine.Length as usize;
        unsafe {
            std::ptr::write_bytes(
                params.CommandLine.Buffer as *mut u8,
                0u8,
                byte_len + std::mem::size_of::<u16>(),
            );
        }
        params.CommandLine.Length = 0;
        params.CommandLine.MaximumLength = 0;
    }

    Ok(())
}
