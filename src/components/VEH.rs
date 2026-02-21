use std::ffi::c_void;

use tracing::{error, info};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
    RemoveVectoredExceptionHandler,
};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows::core::PCWSTR;

pub struct ArVehGuard {
    handle: Option<*mut c_void>,
}

impl ArVehGuard {
    pub fn start() -> Self {
        let handle = unsafe { AddVectoredExceptionHandler(1, Some(ts_veh_handler)) };

        if handle.is_null() {
            error!("Failed to register vectored exception handler");
            return Self { handle: None };
        }

        info!("Vectored exception handler registered");
        Self {
            handle: Some(handle),
        }
    }
}

impl Drop for ArVehGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            unsafe {
                let _ = RemoveVectoredExceptionHandler(handle);
            }
            info!("Vectored exception handler unregistered");
        }
    }
}

unsafe extern "system" fn ts_veh_handler(info: *mut EXCEPTION_POINTERS) -> i32 {
    if info.is_null() {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let pointers = unsafe { &*info };
    if pointers.ExceptionRecord.is_null() {
        return EXCEPTION_CONTINUE_SEARCH;
    }

    let rec = unsafe { &*pointers.ExceptionRecord };
    let code = rec.ExceptionCode.0 as u32;
    let address = rec.ExceptionAddress as usize;
    let code_name = exception_code_name(code);
    let why = describe_exception(code, rec.NumberParameters, &rec.ExceptionInformation);

    if let Some((module, offset)) = resolve_module_and_offset(address) {
        error!(
            exception_code = format!("0x{code:08X}"),
            exception_name = code_name,
            exception_address = format!("0x{address:016X}"),
            module = %module,
            module_offset = format!("0x{offset:X}"),
            cause = %why,
            "Unhandled exception observed"
        );
    } else {
        error!(
            exception_code = format!("0x{code:08X}"),
            exception_name = code_name,
            exception_address = format!("0x{address:016X}"),
            cause = %why,
            "Unhandled exception observed"
        );
    }

    #[cfg(target_arch = "x86_64")]
    if !pointers.ContextRecord.is_null() {
        let ctx = unsafe { &*pointers.ContextRecord };
        error!(
            rip = format!("0x{:016X}", ctx.Rip),
            rsp = format!("0x{:016X}", ctx.Rsp),
            rbp = format!("0x{:016X}", ctx.Rbp),
            rax = format!("0x{:016X}", ctx.Rax),
            rbx = format!("0x{:016X}", ctx.Rbx),
            rcx = format!("0x{:016X}", ctx.Rcx),
            rdx = format!("0x{:016X}", ctx.Rdx),
            rsi = format!("0x{:016X}", ctx.Rsi),
            rdi = format!("0x{:016X}", ctx.Rdi),
            "Exception CPU context"
        );
    }

    EXCEPTION_CONTINUE_SEARCH
}

fn exception_code_name(code: u32) -> &'static str {
    match code {
        0xC0000005 => "STATUS_ACCESS_VIOLATION",
        0xC000001D => "STATUS_ILLEGAL_INSTRUCTION",
        0xC0000094 => "STATUS_INTEGER_DIVIDE_BY_ZERO",
        0xC00000FD => "STATUS_STACK_OVERFLOW",
        0xC0000409 => "STATUS_STACK_BUFFER_OVERRUN",
        0x80000003 => "STATUS_BREAKPOINT",
        0x80000004 => "STATUS_SINGLE_STEP",
        0xC000008C => "STATUS_ARRAY_BOUNDS_EXCEEDED",
        0xC0000096 => "STATUS_PRIVILEGED_INSTRUCTION",
        _ => "STATUS_UNKNOWN",
    }
}

fn describe_exception(code: u32, nparams: u32, info: &[usize; 15]) -> String {
    match code {
        0xC0000005 => {
            let op = if nparams >= 1 { info[0] } else { usize::MAX };
            let at = if nparams >= 2 { info[1] } else { 0 };
            let kind = match op {
                0 => "read",
                1 => "write",
                8 => "execute",
                _ => "unknown",
            };
            format!("Access violation during {kind} at 0x{at:016X}")
        }
        0xC000001D => "Illegal/unsupported instruction executed".to_string(),
        0xC0000094 => "Integer divide by zero".to_string(),
        0xC00000FD => "Stack overflow (likely recursion/deep stack use)".to_string(),
        0xC0000409 => "Stack buffer overrun / fast-fail".to_string(),
        0x80000003 => "Breakpoint trap".to_string(),
        0x80000004 => "Single-step trap".to_string(),
        0xC000008C => "Array bounds exceeded".to_string(),
        0xC0000096 => "Privileged instruction in user mode".to_string(),
        _ => "Unknown exception type".to_string(),
    }
}

fn resolve_module_and_offset(address: usize) -> Option<(String, usize)> {
    let mut module = HMODULE::default();
    let flags =
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT;

    unsafe {
        if GetModuleHandleExW(flags, PCWSTR(address as *const u16), &mut module).is_err() {
            return None;
        }
    }

    let mut buf = vec![0u16; 1024];
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
    if len == 0 {
        return None;
    }

    buf.truncate(len);
    let name = String::from_utf16_lossy(&buf);
    let base = module.0 as usize;

    Some((name, address.saturating_sub(base)))
}
