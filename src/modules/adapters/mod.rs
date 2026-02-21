mod adapter;
mod network;
mod profile_xml;
mod registry;
mod types;
mod util;
mod wifi;
mod wired;

use tracing::info;
use wired::spoof_adapters;

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
        let Some(path) = registry::find_adapter_registry_path(&adapter.guid) else {
            continue;
        };
        if let Some(mac) = registry::get_network_address(&path) {
            out.push((adapter.guid, mac));
        }
    }
    out
}

pub fn ArSpoofMAC(spoof_connected_adapters: bool) {
    info!("Starting MAC spoofing");

    info!(spoof_connected_adapters, "Processing adapters");
    spoof_adapters(spoof_connected_adapters);

    info!("MAC spoofing complete");
}
