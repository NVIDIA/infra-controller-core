use std::collections::HashMap;

use uuid::Uuid;

use super::{Report, ValidationJob};
use crate::client::NiccClient;
use crate::error::RvsError;
use crate::partitions::Partitions;
use crate::rack::Tray;

/// Convert filtered partitions into validation jobs.
///
/// Full preparation pipeline per partition:
///   1. assign_run_id   - stamp every tray with a consistent rv.run-id
///   2. allocate_instances - boot a validation OS instance on each tray
///   3. wait_for_boot   - block until every instance is ready
pub async fn plan(
    partitions: Partitions,
    nicc: &NiccClient,
    os_uri: &str,
) -> Result<Vec<ValidationJob>, RvsError> {
    assign_run_id(&partitions.all, nicc).await?;
    allocate_instances(&partitions.all, os_uri, nicc).await?;
    wait_for_boot(&partitions.all, nicc).await?;
    Ok(vec![ValidationJob {
        trays: partitions.all.into_values().collect(),
    }])
}

/// Ensure every tray carries a consistent `rv.run-id`.
///
/// If all trays already share the same value it is reused. Otherwise a fresh
/// UUID is generated and written to every tray via `update_rv_labels`.
async fn assign_run_id(trays: &HashMap<String, Tray>, nicc: &NiccClient) -> Result<(), RvsError> {
    match existing_run_id(trays) {
        Some(id) => {
            tracing::info!(run_id = %id, "validation: reusing existing run ID");
        }
        None => {
            let id = Uuid::new_v4().to_string();
            tracing::info!(run_id = %id, "validation: assigning new run ID");
            for (tray_id, tray) in trays {
                let mut labels = tray.rv_labels.clone();
                labels.insert("rv.run-id".to_string(), id.clone());
                nicc.update_rv_labels(tray_id, &labels).await?;
            }
        }
    }
    Ok(())
}

/// Allocate a validation OS instance on each tray in the partition.
///
/// TODO[#416]: stub - wire in nicc.allocate_machine_instance per tray and collect
/// instance IDs for boot tracking. ValidationJob will carry them once expanded.
async fn allocate_instances(
    _trays: &HashMap<String, Tray>,
    _os_uri: &str,
    _nicc: &NiccClient,
) -> Result<(), RvsError> {
    let () = std::future::ready(()).await; // phantom await: keeps async sig for future wiring
    Ok(())
}

/// Wait until every allocated instance has booted and reached READY state.
///
/// TODO[#416]: stub - wire in polling loop with exponential backoff and timeout once
/// allocate_instances populates instance IDs on ValidationJob.
async fn wait_for_boot(_trays: &HashMap<String, Tray>, _nicc: &NiccClient) -> Result<(), RvsError> {
    let () = std::future::ready(()).await; // phantom await: keeps async sig for future wiring
    Ok(())
}

/// Run validation against a single job and produce a report.
///
/// Stub: counts trays in the partition as a stand-in for real validation output.
pub async fn validate_partition(job: ValidationJob) -> Result<Report, RvsError> {
    let trays_cnt = job.trays.len() as u32;
    tracing::info!(trays_cnt, "validation: partition validated (stub)");
    Ok(Report { trays_cnt })
}

/// Submit a completed report.
///
/// Stub: prints tray count to console.
pub async fn submit_report(report: Report) -> Result<(), RvsError> {
    println!("validation report: trays_cnt={}", report.trays_cnt);
    Ok(())
}

/// Return the shared run ID if every tray already carries the same `rv.run-id`.
fn existing_run_id(trays: &HashMap<String, Tray>) -> Option<String> {
    let mut ids = trays.values().filter_map(|t| t.rv_labels.get("rv.run-id"));
    let first = ids.next()?.clone();
    if ids.all(|id| *id == first) {
        Some(first)
    } else {
        None
    }
}
