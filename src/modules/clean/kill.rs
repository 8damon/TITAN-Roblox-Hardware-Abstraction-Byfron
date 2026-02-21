#![allow(clippy::manual_c_str_literals, clippy::upper_case_acronyms)]

use std::ffi::c_void;
use std::ptr;

use windows::Win32::Foundation::{CloseHandle, HANDLE, NTSTATUS};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::System::Threading::{
    OpenProcess, OpenThread, PROCESS_QUERY_INFORMATION, PROCESS_TERMINATE, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, SuspendThread, THREAD_SUSPEND_RESUME, TerminateProcess,
};
use windows::core::PCSTR;

type NtUnmapViewOfSection = unsafe extern "system" fn(HANDLE, *mut c_void) -> NTSTATUS;

type NtQueryInformationProcess =
    unsafe extern "system" fn(HANDLE, u32, *mut c_void, u32, *mut u32) -> NTSTATUS;

#[repr(C)]
struct PROCESS_BASIC_INFORMATION {
    ExitStatus: NTSTATUS,
    PebBaseAddress: *mut c_void,
    AffinityMask: usize,
    BasePriority: i32,
    UniqueProcessId: usize,
    InheritedFromUniqueProcessId: usize,
}

#[repr(C)]
struct PEB {
    _reserved: [u8; 0x10],
    ImageBaseAddress: *mut c_void,
}

pub fn ArKillProcess(process_name: &str) -> bool {
    unsafe {
        // Resolve ntdll
        let ntdll = match GetModuleHandleA(PCSTR(b"ntdll.dll\0".as_ptr())) {
            Ok(m) => m,
            Err(_) => return false,
        };

        // Resolve NT exports
        let nt_unmap_raw = match GetProcAddress(ntdll, PCSTR(b"NtUnmapViewOfSection\0".as_ptr())) {
            Some(p) => p,
            None => return false,
        };

        let nt_query_raw =
            match GetProcAddress(ntdll, PCSTR(b"NtQueryInformationProcess\0".as_ptr())) {
                Some(p) => p,
                None => return false,
            };

        let nt_unmap: NtUnmapViewOfSection = std::mem::transmute(nt_unmap_raw);

        let nt_query: NtQueryInformationProcess = std::mem::transmute(nt_query_raw);

        // Enumerate processes
        let snapshot = match CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) {
            Ok(s) => s,
            Err(_) => return false,
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };

        let mut success = false;

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile)
                    .trim_matches(char::from(0))
                    .to_string();

                if name.eq_ignore_ascii_case(process_name) {
                    let pid = entry.th32ProcessID;

                    let access = PROCESS_QUERY_INFORMATION
                        | PROCESS_VM_READ
                        | PROCESS_VM_OPERATION
                        | PROCESS_TERMINATE;

                    if let Ok(h_proc) = OpenProcess(access, false, pid) {
                        suspend_threads(pid);

                        let mut pbi: PROCESS_BASIC_INFORMATION = std::mem::zeroed();

                        let status = nt_query(
                            h_proc,
                            0,
                            &mut pbi as *mut _ as _,
                            std::mem::size_of::<PROCESS_BASIC_INFORMATION>() as u32,
                            ptr::null_mut(),
                        );

                        if status.0 == 0 && !pbi.PebBaseAddress.is_null() {
                            let mut remote_peb: PEB = std::mem::zeroed();

                            let mut bytes_read = 0;

                            if ReadProcessMemory(
                                h_proc,
                                pbi.PebBaseAddress,
                                &mut remote_peb as *mut _ as _,
                                std::mem::size_of::<PEB>(),
                                Some(&mut bytes_read),
                            )
                            .is_ok()
                                && !remote_peb.ImageBaseAddress.is_null()
                            {
                                let _ = nt_unmap(h_proc, remote_peb.ImageBaseAddress);
                            }
                        }

                        let _ = TerminateProcess(h_proc, 0);
                        let _ = CloseHandle(h_proc);

                        success = true;
                    }
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        success
    }
}

fn suspend_threads(pid: u32) {
    unsafe {
        if let Ok(thread_snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) {
            let mut te = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };

            if Thread32First(thread_snapshot, &mut te).is_ok() {
                loop {
                    if te.th32OwnerProcessID == pid
                        && let Ok(h_thread) =
                            OpenThread(THREAD_SUSPEND_RESUME, false, te.th32ThreadID)
                    {
                        let _ = SuspendThread(h_thread);
                        let _ = CloseHandle(h_thread);
                    }

                    if Thread32Next(thread_snapshot, &mut te).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(thread_snapshot);
        }
    }
}
