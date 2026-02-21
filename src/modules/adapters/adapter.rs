use std::collections::HashSet;

use super::types::AdapterInfo;
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::*;
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;

pub fn enumerate_adapters() -> Vec<AdapterInfo> {
    let mut adapters = Vec::new();
    let mut buf_size = 16 * 1024u32;
    let mut buf = vec![0u8; buf_size as usize];

    let mut ptr = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
    let flags = GAA_FLAG_INCLUDE_ALL_INTERFACES | GAA_FLAG_INCLUDE_ALL_COMPARTMENTS;

    let mut rc = unsafe { GetAdaptersAddresses(0, flags, None, Some(ptr), &mut buf_size) };
    if rc == ERROR_BUFFER_OVERFLOW.0 {
        buf.resize(buf_size as usize, 0);
        ptr = buf.as_mut_ptr() as *mut _;
        rc = unsafe { GetAdaptersAddresses(0, flags, None, Some(ptr), &mut buf_size) };
    }
    if rc != NO_ERROR.0 {
        return adapters;
    }

    let mut cur = ptr;
    while !cur.is_null() {
        unsafe {
            let if_type = (*cur).IfType;
            if if_type == 245 {
                // loopback
                cur = (*cur).Next;
                continue;
            }

            let guid = if !(*cur).AdapterName.0.is_null() {
                std::ffi::CStr::from_ptr((*cur).AdapterName.0 as *const i8)
                    .to_string_lossy()
                    .into_owned()
            } else {
                String::new()
            };

            let name = if !(*cur).FriendlyName.0.is_null() {
                (*cur).FriendlyName.to_string().unwrap_or_default()
            } else {
                String::new()
            };

            adapters.push(AdapterInfo {
                friendly_name: name,
                guid,
                if_type,
            });

            cur = (*cur).Next;
        }
    }

    adapters
}

pub fn enumerate_connected_wifi_ethernet_guids() -> HashSet<String> {
    let mut active = HashSet::new();
    let mut buf_size = 16 * 1024u32;
    let mut buf = vec![0u8; buf_size as usize];

    let mut ptr = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
    let flags = GAA_FLAG_INCLUDE_ALL_INTERFACES | GAA_FLAG_INCLUDE_ALL_COMPARTMENTS;

    let mut rc = unsafe { GetAdaptersAddresses(0, flags, None, Some(ptr), &mut buf_size) };
    if rc == ERROR_BUFFER_OVERFLOW.0 {
        buf.resize(buf_size as usize, 0);
        ptr = buf.as_mut_ptr() as *mut _;
        rc = unsafe { GetAdaptersAddresses(0, flags, None, Some(ptr), &mut buf_size) };
    }
    if rc != NO_ERROR.0 {
        return active;
    }

    let mut cur = ptr;
    while !cur.is_null() {
        unsafe {
            let if_type = (*cur).IfType;
            let is_target_media = if_type == 6 || if_type == 71;
            let is_up = (*cur).OperStatus == IfOperStatusUp;
            let has_unicast = !(*cur).FirstUnicastAddress.is_null();

            if is_target_media && is_up && has_unicast && !(*cur).AdapterName.0.is_null() {
                let guid = std::ffi::CStr::from_ptr((*cur).AdapterName.0 as *const i8)
                    .to_string_lossy()
                    .into_owned()
                    .to_ascii_lowercase();
                if !guid.is_empty() {
                    active.insert(guid);
                }
            }

            cur = (*cur).Next;
        }
    }

    active
}
