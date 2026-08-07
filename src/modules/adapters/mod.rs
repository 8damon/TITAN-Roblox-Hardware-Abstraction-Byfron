mod adapter;
mod net_registry;
mod network;
mod profile_xml;
mod types;
mod util;
mod wifi;
mod wired;

use tracing::{error, info};
use wired::spoof_adapters;

use crate::modules::adapters::network::flush_dns_cache;
#[allow(unused_imports)]
pub use network::{
    ArCaptureActiveNetworkSnapshot, ArLogNetworkPreflight, ArNetworkSnapshot,
    ArVerifyNetworkPreservedAfterMacSpoof,
};

pub fn ArSnapshotMacTargets() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for adapter in adapter::enumerate_adapters() {
        if adapter.if_type == 71 || adapter.if_type == 245 {
            continue;
        }
        if adapter.guid.is_empty() {
            continue;
        }
        let Some(path) = net_registry::find_adapter_registry_path(&adapter.guid) else {
            continue;
        };
        if let Some(mac) = net_registry::get_network_address(&path) {
            out.push((adapter.guid, mac));
        }
    }
    out
}

pub fn ArSpoofMAC(spoof_connected_adapters: bool) {
    info!("Starting MAC spoofing");

    info!(spoof_connected_adapters, "Processing adapters");
    spoof_adapters(spoof_connected_adapters);

    match flush_dns_cache() {
        Ok(true) => {
            info!("DNS cache flushed");
        }
        Err(e) => {
            error!("DNS flush failed: {}", e);
        }
        _ => {}
    }

    info!("MAC spoofing complete");
}
