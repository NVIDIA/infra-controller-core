mod io;
use std::collections::HashMap;

pub use io::NiccClient;
use rpc::forge::{Machine, Rack};

/// NVLink fields extracted from gRPC Machine.
#[derive(Debug)]
pub struct TrayNvlData {
    /// NVL domain this tray belongs to.
    pub domain_uuid: Option<String>,
    /// GPU count reported via NVLink info.
    pub gpu_count: u32,
}

/// InfiniBand fields extracted from gRPC Machine.
#[derive(Debug)]
pub struct TrayIbData {
    /// Fabric IDs observed across IB interfaces (used as partition keys).
    pub fabric_ids: Vec<String>,
    /// Total IB interfaces on this machine.
    pub port_count: u32,
    /// IB interfaces with active LID.
    pub active_port_count: u32,
}

/// Intermediate representation of a gRPC Machine.
#[derive(Debug)]
pub struct TrayData {
    /// Machine ID as string.
    pub id: String,
    /// Rack-validation labels (`rv.*`) from machine metadata.
    pub rv_labels: HashMap<String, String>,
    /// NVLink data, if machine has NVLink info.
    pub nvl: Option<TrayNvlData>,
    /// InfiniBand data, if machine has IB status.
    pub ib: Option<TrayIbData>,
}

/// Extract TrayData from gRPC Machine.
impl From<Machine> for TrayData {
    fn from(value: Machine) -> Self {
        let id = value
            .id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_default();

        let nvl = value.nvlink_info.map(|info| TrayNvlData {
            domain_uuid: info.domain_uuid.as_ref().map(|u| u.to_string()),
            gpu_count: info.gpus.len() as u32,
        });

        let ib = value.ib_status.map(|status| {
            let port_count = status.ib_interfaces.len() as u32;
            let active_port_count = status
                .ib_interfaces
                .iter()
                .filter(|iface| matches!(iface.lid, Some(lid) if lid != 0 && lid != 0xffff))
                .count() as u32;
            let fabric_ids = status
                .ib_interfaces
                .iter()
                .filter_map(|iface| iface.fabric_id.clone())
                .collect();
            TrayIbData {
                fabric_ids,
                port_count,
                active_port_count,
            }
        });

        let rv_labels = value
            .metadata
            .map(|m| {
                m.labels
                    .into_iter()
                    .filter(|l| l.key.starts_with("rv."))
                    .filter_map(|l| l.value.map(|v| (l.key, v)))
                    .collect()
            })
            .unwrap_or_default();

        Self {
            id,
            rv_labels,
            nvl,
            ib,
        }
    }
}

/// Intermediate representation of a gRPC Rack.
#[derive(Debug)]
pub struct RackData {
    /// Rack ID as string.
    pub id: String,
    /// Rack lifecycle state.
    pub state: String,
    /// Machine IDs of compute trays in this rack.
    pub compute_tray_ids: Vec<String>,
}

/// Extract RackData from gRPC Rack.
impl From<Rack> for RackData {
    fn from(value: Rack) -> Self {
        Self {
            id: value
                .id
                .as_ref()
                .map(|id| id.to_string())
                .unwrap_or_default(),
            state: value.rack_state,
            compute_tray_ids: value
                .compute_trays
                .into_iter()
                .map(|id| id.to_string())
                .collect(),
        }
    }
}
