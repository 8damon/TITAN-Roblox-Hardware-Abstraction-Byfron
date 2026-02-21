use super::adapter::{enumerate_adapters, enumerate_connected_wifi_ethernet_guids};
use super::registry::{find_adapter_registry_path, set_network_address};
use super::util::{bounce_adapter, gen_random_mac};

use tracing::{debug, error, info, warn};

use std::collections::HashSet;
use std::thread;

pub fn spoof_adapters(spoof_connected_adapters: bool) {
    info!(spoof_connected_adapters, "Starting adapter spoof");
    let enable_bounce = env_flag("TSRS_ADAPTER_BOUNCE", false);

    let adapters = enumerate_adapters();
    let active_guids: HashSet<String> = if spoof_connected_adapters {
        HashSet::new()
    } else {
        enumerate_connected_wifi_ethernet_guids()
    };

    if adapters.is_empty() {
        info!("No adapters found");
        return;
    }

    let mut handles = Vec::new();

    for adapter in adapters {
        if adapter.if_type == 245 {
            continue;
        }

        if adapter.guid.is_empty() || adapter.friendly_name.is_empty() {
            continue;
        }

        let guid_key = adapter.guid.to_ascii_lowercase();
        if !spoof_connected_adapters
            && (adapter.if_type == 6 || adapter.if_type == 71)
            && active_guids.contains(&guid_key)
        {
            info!(
                name = %adapter.friendly_name,
                guid = %adapter.guid,
                if_type = adapter.if_type,
                "Skipping active network adapter"
            );
            continue;
        }

        handles.push(thread::spawn(move || {
            debug!(name = %adapter.friendly_name, guid = %adapter.guid, "Processing adapter");

            let reg_path = match find_adapter_registry_path(&adapter.guid) {
                Some(p) => p,
                None => {
                    warn!(name = %adapter.friendly_name, "Registry path not found");
                    return;
                }
            };

            let mac = gen_random_mac();

            if !set_network_address(&reg_path, &mac) {
                error!(name = %adapter.friendly_name, "Failed to set NetworkAddress");
                return;
            }

            info!(name = %adapter.friendly_name, mac = %mac, "MAC updated");

            if enable_bounce {
                if !bounce_adapter(&adapter.friendly_name) {
                    warn!(name = %adapter.friendly_name, "Adapter bounce failed");
                }
            } else {
                debug!(
                    name = %adapter.friendly_name,
                    "Adapter bounce skipped (TSRS_ADAPTER_BOUNCE=false)"
                );
            }
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    info!("Adapter spoof complete");
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let s = v.trim().to_ascii_lowercase();
            matches!(s.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => default,
    }
}
