// wmi.rs

use crate::components::generator::{
    gen_device_id, gen_guid, gen_pnp_id, gen_processor_id, gen_serial,
};
use tracing::{error, info, warn};
use windows::Win32::Foundation::RPC_E_TOO_LATE;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    CoInitializeSecurity, CoSetProxyBlanket, CoUninitialize, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL,
    RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Variant::{
    VARENUM, VARIANT, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VariantClear,
};
use windows::Win32::System::Wmi::{
    IWbemLocator, IWbemServices, WBEM_E_ACCESS_DENIED, WBEM_E_NOT_SUPPORTED, WBEM_E_READ_ONLY,
    WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE, WbemLocator,
};
use windows::core::{BSTR, Error, PCWSTR, w};

struct ComInit;

impl ComInit {
    fn new() -> Self {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            match CoInitializeSecurity(
                None,
                -1,
                None,
                None,
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
                None,
            ) {
                Ok(()) => {}
                Err(e) if e.code() == RPC_E_TOO_LATE => {}
                Err(e) => warn!("CoInitializeSecurity failed: {:?}", e),
            }
        }
        Self
    }
}

impl Drop for ComInit {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn connect_wmi() -> Option<IWbemServices> {
    let locator: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()? };

    let namespace = BSTR::from("ROOT\\CIMV2");
    let empty = BSTR::new();

    let service = unsafe {
        locator
            .ConnectServer(&namespace, &empty, &empty, &empty, 0, &empty, None)
            .ok()?
    };

    unsafe {
        CoSetProxyBlanket(
            &service,
            RPC_C_AUTHN_WINNT,
            RPC_C_AUTHZ_NONE,
            PCWSTR::null(),
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
        .ok()?;
    }

    Some(service)
}

fn make_bstr_variant(s: &str) -> VARIANT {
    let mut var: VARIANT = unsafe { std::mem::zeroed() };
    var.Anonymous.Anonymous = std::mem::ManuallyDrop::new(VARIANT_0_0 {
        vt: VARENUM(VT_BSTR.0),
        wReserved1: 0,
        wReserved2: 0,
        wReserved3: 0,
        Anonymous: VARIANT_0_0_0 {
            bstrVal: std::mem::ManuallyDrop::new(BSTR::from(s)),
        },
    });
    var
}

fn is_expected_wmi_write_failure(e: &Error) -> bool {
    let code = e.code().0;
    code == WBEM_E_READ_ONLY.0 || code == WBEM_E_ACCESS_DENIED.0 || code == WBEM_E_NOT_SUPPORTED.0
}

fn spoof_single_property(class_name: &str, query: &str, property: PCWSTR, value: &str) -> bool {
    let Some(service) = connect_wmi() else {
        error!("Failed to connect to WMI namespace for {}", class_name);
        return false;
    };

    let query_lang = BSTR::from("WQL");
    let query_text = BSTR::from(query);

    let enumerator = match unsafe {
        service.ExecQuery(
            &query_lang,
            &query_text,
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        )
    } {
        Ok(e) => e,
        Err(e) => {
            error!("ExecQuery failed for {}: {:?}", class_name, e);
            return false;
        }
    };

    let mut seen = 0u32;
    let mut updated = 0u32;
    let mut expected_failures = 0u32;
    let mut hard_failures = 0u32;

    loop {
        let mut objs = [None; 1];
        let mut returned: u32 = 0;

        let next = unsafe { enumerator.Next(WBEM_INFINITE, &mut objs, &mut returned) };
        if let Err(e) = next.ok() {
            error!("Enumerator.Next failed for {}: {:?}", class_name, e);
            return false;
        }
        if returned == 0 {
            break;
        }

        seen += 1;
        let Some(p_obj) = objs[0].take() else {
            continue;
        };

        let mut var = make_bstr_variant(value);
        let put_res = unsafe { p_obj.Put(property, 0, &var, 0) };
        unsafe {
            let _ = VariantClear(&mut var);
        }

        match put_res {
            Ok(()) => updated += 1,
            Err(e) if is_expected_wmi_write_failure(&e) => expected_failures += 1,
            Err(e) => {
                hard_failures += 1;
                warn!("{}::Put failed with {:?}", class_name, e);
            }
        }
    }

    if seen == 0 {
        warn!("No instances found for {}", class_name);
        return false;
    }

    if updated > 0 {
        info!(
            "Spoofed {} ({}/{} instances updated)",
            class_name, updated, seen
        );
        return true;
    }

    if expected_failures > 0 && hard_failures == 0 {
        warn!(
            "{} is exposed as read-only/locked via WMI provider; skipping",
            class_name
        );
        return true;
    }

    false
}

pub fn ArSpoofWMI() -> bool {
    info!("Starting WMI spoofing");
    let _com = ComInit::new();

    let mut success = true;

    if !spoof_single_property(
        "Win32_ComputerSystemProduct",
        "SELECT * FROM Win32_ComputerSystemProduct",
        w!("UUID"),
        &gen_guid(),
    ) {
        error!("Win32_ComputerSystemProduct spoof failed");
        success = false;
    }

    if !spoof_single_property(
        "Win32_PhysicalMemory",
        "SELECT * FROM Win32_PhysicalMemory",
        w!("SerialNumber"),
        &gen_serial(),
    ) {
        error!("Win32_PhysicalMemory spoof failed");
        success = false;
    }

    let disk_serial = spoof_single_property(
        "Win32_DiskDrive",
        "SELECT * FROM Win32_DiskDrive",
        w!("SerialNumber"),
        &gen_serial(),
    );
    let disk_pnp = spoof_single_property(
        "Win32_DiskDrive",
        "SELECT * FROM Win32_DiskDrive",
        w!("PNPDeviceID"),
        &gen_pnp_id(),
    );
    let disk_device = spoof_single_property(
        "Win32_DiskDrive",
        "SELECT * FROM Win32_DiskDrive",
        w!("DeviceID"),
        &gen_device_id(),
    );
    if !(disk_serial || disk_pnp || disk_device) {
        error!("Win32_DiskDrive spoof failed");
        success = false;
    }

    if !spoof_single_property(
        "Win32_BIOS",
        "SELECT * FROM Win32_BIOS",
        w!("SerialNumber"),
        &gen_serial(),
    ) {
        error!("Win32_BIOS spoof failed");
        success = false;
    }

    if !spoof_single_property(
        "Win32_BaseBoard",
        "SELECT * FROM Win32_BaseBoard",
        w!("SerialNumber"),
        &gen_serial(),
    ) {
        error!("Win32_BaseBoard spoof failed");
        success = false;
    }

    if !spoof_single_property(
        "Win32_Processor",
        "SELECT * FROM Win32_Processor",
        w!("ProcessorId"),
        &gen_processor_id(),
    ) {
        error!("Win32_Processor spoof failed");
        success = false;
    }

    if !spoof_single_property(
        "Win32_VideoController",
        "SELECT * FROM Win32_VideoController",
        w!("PNPDeviceID"),
        &gen_pnp_id(),
    ) {
        error!("Win32_VideoController spoof failed");
        success = false;
    }

    info!(
        "WMI spoofing finished → {}",
        if success { "success" } else { "partial/failed" }
    );
    success
}
