use std::collections::HashMap;

use rpc::forge::{
    GetRackRequest, Instance, InstanceAllocationRequest, InstanceConfig, Label,
    MachineMetadataUpdateRequest, MachinesByIdsRequest, Metadata,
};
use rpc::forge_api_client::ForgeApiClient;
use rpc::forge_tls_client::ApiConfig;
use rpc::protos::forge::{InstancesByIdsRequest, OperatingSystem, operating_system};

use super::{RackData, TrayData};
use crate::error::RvsError;

/// NICC gRPC client wrapper -- translates gRPC responses into IR types.
pub struct NiccClient {
    inner: ForgeApiClient,
}

impl NiccClient {
    /// Construct from API config.
    pub fn new(api_config: &ApiConfig<'_>) -> Self {
        Self {
            inner: ForgeApiClient::new(api_config),
        }
    }

    /// Fetch all racks from NICC -> Vec<RackData>.
    pub async fn get_racks(&self) -> Result<Vec<RackData>, RvsError> {
        let response = self.inner.get_rack(GetRackRequest { id: None }).await?;
        Ok(response.rack.into_iter().map(RackData::from).collect())
    }

    /// Replace all `rv.*` labels on a machine with the provided map.
    ///
    /// RVS owns all `rv.*` labels, so replacing them wholesale is safe within
    /// its scope. Non-`rv.*` labels on the machine are not affected because
    /// the caller is responsible for passing the correct complete set.
    pub async fn update_rv_labels(
        &self,
        tray_id: &str,
        labels: &HashMap<String, String>,
    ) -> Result<(), RvsError> {
        let machine_id = tray_id.parse()?;
        let label_protos = labels
            .iter()
            .map(|(k, v)| Label {
                key: k.clone(),
                value: Some(v.clone()),
            })
            .collect();
        self.inner
            .update_machine_metadata(MachineMetadataUpdateRequest {
                machine_id: Some(machine_id),
                if_version_match: None,
                metadata: Some(Metadata {
                    name: String::new(),
                    description: String::new(),
                    labels: label_protos,
                }),
            })
            .await?;
        Ok(())
    }

    /// Allocate a validation instance on a single machine.
    #[allow(dead_code)]
    ///
    /// The OS is identified by `os_uri` from the scenario file. Until RVS can
    /// resolve the URI to a NICC OS image UUID, `os_image_id` is stubbed with
    /// a nil UUID - the call will fail in production until this is wired up.
    pub async fn allocate_machine_instance(
        &self,
        machine_id: &str,
        os_uri: &str,
    ) -> Result<String, RvsError> {
        let machine_id = machine_id.parse()?;
        tracing::info!(%os_uri, "validation: allocating instance (os_image_id stubbed)");
        let response = self
            .inner
            .allocate_instance(InstanceAllocationRequest {
                machine_id: Some(machine_id),
                config: Some(InstanceConfig {
                    os: Some(OperatingSystem {
                        // TODO[#416]: resolve os_uri to a NICC OS image UUID via ListOsImage /
                        //       an external registry lookup. For now, nil UUID is a known
                        //       stub that will be replaced once image resolution is wired.
                        variant: Some(operating_system::Variant::OsImageId(rpc::common::Uuid {
                            value: "00000000-0000-0000-0000-000000000000".to_string(),
                        })),
                        phone_home_enabled: false,
                        run_provisioning_instructions_on_every_boot: false,
                        user_data: None,
                    }),
                    tenant: None,
                    network: None,
                    infiniband: None,
                    network_security_group_id: None,
                    dpu_extension_services: None,
                    nvlink: None,
                }),
                instance_id: None,
                instance_type_id: None,
                metadata: None,
                allow_unhealthy_machine: false,
            })
            .await?;
        Ok(response.id.map(|id| id.to_string()).unwrap_or_default())
    }

    /// Fetch current state of instances by their IDs.
    #[allow(dead_code)]
    pub async fn get_instances(&self, instance_ids: &[String]) -> Result<Vec<Instance>, RvsError> {
        let ids = instance_ids
            .iter()
            .map(|id| {
                id.parse()
                    .map_err(|e: uuid::Error| RvsError::InvalidId(e.to_string()))
            })
            .collect::<Result<_, _>>()?;
        let response = self
            .inner
            .find_instances_by_ids(InstancesByIdsRequest { instance_ids: ids })
            .await?;
        Ok(response.instances)
    }

    /// Fetch machines for a rack's compute trays -> Vec<TrayData>. Chunked at 50.
    pub async fn get_machines(&self, rack: &RackData) -> Result<Vec<TrayData>, RvsError> {
        let mut trays = Vec::with_capacity(rack.compute_tray_ids.len());

        for chunk in rack.compute_tray_ids.chunks(50) {
            let machine_ids = chunk
                .iter()
                .map(|id| id.parse())
                .collect::<Result<_, _>>()
                .map_err(RvsError::from)?;

            let response = self
                .inner
                .find_machines_by_ids(MachinesByIdsRequest {
                    machine_ids,
                    include_history: false,
                })
                .await?;

            trays.extend(response.machines.into_iter().map(TrayData::from));
        }

        Ok(trays)
    }
}
