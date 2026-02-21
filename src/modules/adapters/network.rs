use std::time::{Duration, Instant};

use tracing::{info, warn};
use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS, NO_ERROR};
use windows::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_INCLUDE_ALL_COMPARTMENTS, GAA_FLAG_INCLUDE_ALL_INTERFACES, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::WiFi::{
    WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO_LIST, WlanCloseHandle, WlanEnumInterfaces,
    WlanFreeMemory, WlanOpenHandle, WlanQueryInterface, wlan_interface_state_connected,
    wlan_intf_opcode_current_connection,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArNetworkSnapshot {
    pub network_type: String,
    pub band: Option<String>,
    pub speed_mbps: Option<u64>,
}

pub fn ArLogNetworkPreflight() -> Option<ArNetworkSnapshot> {
    let snap = ArCaptureActiveNetworkSnapshot();
    if let Some(s) = &snap {
        info!(
            network_type = %s.network_type,
            band = s.band.as_deref().unwrap_or("n/a"),
            speed_mbps = s.speed_mbps.unwrap_or(0),
            "Network preflight"
        );
    } else {
        warn!("Network preflight: no active Wi-Fi/Ethernet connection detected");
    }
    snap
}

pub fn ArCaptureActiveNetworkSnapshot() -> Option<ArNetworkSnapshot> {
    capture_active_wifi_snapshot().or_else(capture_active_ethernet_snapshot)
}

pub fn ArVerifyNetworkPreservedAfterMacSpoof(
    before: &ArNetworkSnapshot,
    wait_timeout: Duration,
) -> bool {
    let start = Instant::now();
    let mut last_seen: Option<ArNetworkSnapshot> = None;

    while start.elapsed() < wait_timeout {
        if let Some(after) = ArCaptureActiveNetworkSnapshot() {
            if &after == before {
                info!(
                    network_type = %after.network_type,
                    band = after.band.as_deref().unwrap_or("n/a"),
                    speed_mbps = after.speed_mbps.unwrap_or(0),
                    "Post-MAC network verification passed"
                );
                return true;
            }
            last_seen = Some(after);
        }
        std::thread::sleep(Duration::from_millis(500));
    }

    if let Some(after) = last_seen {
        warn!(
            before_type = %before.network_type,
            before_band = before.band.as_deref().unwrap_or("n/a"),
            before_speed_mbps = before.speed_mbps.unwrap_or(0),
            after_type = %after.network_type,
            after_band = after.band.as_deref().unwrap_or("n/a"),
            after_speed_mbps = after.speed_mbps.unwrap_or(0),
            "Post-MAC network verification failed: connection characteristics changed"
        );
    } else {
        warn!("Post-MAC network verification failed: no active connection snapshot found");
    }

    false
}

fn capture_active_wifi_snapshot() -> Option<ArNetworkSnapshot> {
    unsafe {
        let mut version = 0u32;
        let mut handle = windows::Win32::Foundation::HANDLE::default();
        if WlanOpenHandle(2, None, &mut version, &mut handle) != ERROR_SUCCESS.0 {
            return None;
        }

        let mut list_ptr: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(handle, None, &mut list_ptr) != ERROR_SUCCESS.0 || list_ptr.is_null()
        {
            let _ = WlanCloseHandle(handle, None);
            return None;
        }

        let count = (*list_ptr).dwNumberOfItems;
        for i in 0..count {
            let iface = &(*list_ptr).InterfaceInfo[i as usize];
            if iface.isState != wlan_interface_state_connected {
                continue;
            }

            let mut size = 0u32;
            let mut conn: *mut WLAN_CONNECTION_ATTRIBUTES = std::ptr::null_mut();
            let mut value_type = windows::Win32::NetworkManagement::WiFi::WLAN_OPCODE_VALUE_TYPE(0);

            if WlanQueryInterface(
                handle,
                &iface.InterfaceGuid,
                wlan_intf_opcode_current_connection,
                None,
                &mut size,
                &mut conn as *mut _ as *mut *mut _,
                Some(&mut value_type),
            ) != ERROR_SUCCESS.0
                || conn.is_null()
            {
                continue;
            }

            let assoc = (*conn).wlanAssociationAttributes;
            let band = wifi_band_from_phy_type(assoc.dot11PhyType.0);
            let speed_mbps = sanitize_speed_bps(assoc.ulRxRate as u64);

            let profile_name = {
                let ptr = (*conn).strProfileName.as_ptr();
                if ptr.is_null() {
                    String::new()
                } else {
                    let slice = std::slice::from_raw_parts(ptr, 256);
                    let len = slice.iter().position(|&c| c == 0).unwrap_or(256);
                    String::from_utf16_lossy(&slice[..len])
                }
            };

            WlanFreeMemory(conn as *mut _);
            WlanFreeMemory(list_ptr as *mut _);
            let _ = WlanCloseHandle(handle, None);

            let network_type = if looks_like_hotspot(&profile_name) {
                "Hotspot".to_string()
            } else {
                "Wi-Fi".to_string()
            };

            return Some(ArNetworkSnapshot {
                network_type,
                band,
                speed_mbps,
            });
        }

        WlanFreeMemory(list_ptr as *mut _);
        let _ = WlanCloseHandle(handle, None);
        None
    }
}

fn capture_active_ethernet_snapshot() -> Option<ArNetworkSnapshot> {
    unsafe {
        let mut buf_size = 16 * 1024u32;
        let mut buf = vec![0u8; buf_size as usize];
        let mut ptr = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        let flags = GAA_FLAG_INCLUDE_ALL_INTERFACES | GAA_FLAG_INCLUDE_ALL_COMPARTMENTS;

        let mut rc = GetAdaptersAddresses(0, flags, None, Some(ptr), &mut buf_size);
        if rc == ERROR_BUFFER_OVERFLOW.0 {
            buf.resize(buf_size as usize, 0);
            ptr = buf.as_mut_ptr() as *mut _;
            rc = GetAdaptersAddresses(0, flags, None, Some(ptr), &mut buf_size);
        }
        if rc != NO_ERROR.0 {
            return None;
        }

        let mut best: Option<(u8, u64)> = None;
        let mut cur = ptr;
        while !cur.is_null() {
            let if_type = (*cur).IfType;
            if if_type == 6 {
                let speed_bps = (*cur).TransmitLinkSpeed.max((*cur).ReceiveLinkSpeed);
                let Some(speed_mbps) = sanitize_speed_bps(speed_bps) else {
                    cur = (*cur).Next;
                    continue;
                };

                let name = if !(*cur).FriendlyName.0.is_null() {
                    (*cur).FriendlyName.to_string().unwrap_or_default()
                } else {
                    String::new()
                };

                let has_gateway = !(*cur).FirstGatewayAddress.is_null();
                let has_unicast = !(*cur).FirstUnicastAddress.is_null();
                let virtual_like = looks_like_virtual_adapter(&name);

                // Score priority:
                // 3 = physical-like + gateway + unicast
                // 2 = physical-like + unicast
                // 1 = any active-looking ethernet
                let score = if !virtual_like && has_gateway && has_unicast {
                    3
                } else if !virtual_like && has_unicast {
                    2
                } else if has_unicast {
                    1
                } else {
                    0
                };

                if score > 0 {
                    match best {
                        Some((best_score, best_speed)) => {
                            if score > best_score
                                || (score == best_score && speed_mbps > best_speed)
                            {
                                best = Some((score, speed_mbps));
                            }
                        }
                        None => best = Some((score, speed_mbps)),
                    }
                }
            }
            cur = (*cur).Next;
        }

        best.map(|(_, speed_mbps)| ArNetworkSnapshot {
            network_type: "Ethernet".to_string(),
            band: None,
            speed_mbps: Some(speed_mbps),
        })
    }
}

fn wifi_band_from_phy_type(phy: i32) -> Option<String> {
    match phy {
        5 | 6 => Some("2.4 GHz".to_string()),
        4 | 8 | 9 => Some("5 GHz".to_string()),
        10 | 11 => Some("6 GHz".to_string()),
        7 => Some("2.4/5 GHz".to_string()),
        _ => None,
    }
}

fn looks_like_hotspot(profile_name: &str) -> bool {
    let p = profile_name.to_ascii_lowercase();
    p.contains("hotspot")
        || p.contains("mobile")
        || p.contains("iphone")
        || p.contains("android")
        || p.contains("tether")
}

fn looks_like_virtual_adapter(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("virtual")
        || n.contains("veth")
        || n.contains("vmware")
        || n.contains("hyper-v")
        || n.contains("vswitch")
        || n.contains("tailscale")
        || n.contains("loopback")
        || n.contains("bluetooth")
        || n.contains("npcap")
        || n.contains("wsl")
}

fn sanitize_speed_bps(speed_bps: u64) -> Option<u64> {
    // Windows sometimes reports invalid/unknown speeds as all-bits-set.
    if speed_bps == 0 || speed_bps == u64::MAX {
        return None;
    }

    let mbps = speed_bps / 1_000_000;
    // Guard against nonsensical values from bad adapter reports.
    if mbps == 0 || mbps > 1_000_000 {
        return None;
    }

    Some(mbps)
}
