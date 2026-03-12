use carbide_uuid::machine::MachineIdParseError;
use thiserror::Error;

/// Top-level RVS error type.
#[derive(Debug, Error)]
pub enum RvsError {
    /// gRPC call to NICC failed.
    #[error("NICC RPC error: {0}")]
    Rpc(#[from] tonic::Status),

    /// Tray ID string couldn't be parsed as MachineId.
    #[error("Failed to parse Machine ID: {0}")]
    InvalidMachineId(#[from] MachineIdParseError),

    /// An ID string couldn't be parsed as a UUID-based type.
    #[allow(dead_code)]
    #[error("Failed to parse ID: {0}")]
    InvalidId(String),
}
