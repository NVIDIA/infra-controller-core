/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use mac_address::MacAddress;
use model::address_selection_strategy::AddressSelectionStrategy;
use model::allocation_type::AllocationType;
use model::network_segment::NetworkSegmentType;
use rpc::forge as rpc;
use tonic::{Request, Response, Status};

use crate::api::Api;
use crate::errors::CarbideError;

/// Pre-allocate a machine_interface with a static address so
/// site_explorer can discover the BMC at that IP.
///
/// If the IP is within a managed network prefix, the interface is
/// created on that segment. Otherwise it falls back to the first
/// underlay segment as an anchor for external/BYO IPs.
///
/// Currently "assumes" a BMC, but hey maybe we can use it for
/// other things as time goes on.
pub async fn preallocate_machine_interface(
    txn: &mut sqlx::PgConnection,
    bmc_mac_address: MacAddress,
    bmc_ip: std::net::IpAddr,
) -> Result<(), CarbideError> {
    let segment = match db::network_segment::for_relay(txn, bmc_ip).await? {
        Some(seg) => seg,
        None => {
            let underlay_ids =
                db::network_segment::list_segment_ids(txn, Some(NetworkSegmentType::Underlay))
                    .await?;
            let segment_id = underlay_ids.first().ok_or(CarbideError::NotFoundError {
                kind: "underlay_segment",
                id: "any".to_string(),
            })?;
            db::network_segment::find_by(
                txn,
                db::ObjectColumnFilter::One(db::network_segment::IdColumn, segment_id),
                Default::default(),
            )
            .await?
            .into_iter()
            .next()
            .ok_or(CarbideError::NotFoundError {
                kind: "underlay_segment",
                id: segment_id.to_string(),
            })?
        }
    };

    db::machine_interface::create(
        txn,
        &segment,
        &bmc_mac_address,
        segment.subdomain_id,
        true,
        AddressSelectionStrategy::StaticAddress(bmc_ip),
    )
    .await?;

    tracing::info!(
        %bmc_mac_address,
        %bmc_ip,
        segment_id = %segment.id,
        "Pre-allocated static machine interface"
    );

    Ok(())
}

pub async fn assign_static_address(
    api: &Api,
    request: Request<rpc::AssignStaticAddressRequest>,
) -> Result<Response<rpc::AssignStaticAddressResponse>, CarbideError> {
    let req = request.into_inner();
    let interface_id = req.interface_id.ok_or(CarbideError::InvalidArgument(
        "interface_id is required".into(),
    ))?;
    let ip_address: std::net::IpAddr = req.ip_address.parse()?;

    let mut txn = api.txn_begin().await?;
    let result =
        db::machine_interface_address::assign_static(&mut txn, interface_id, ip_address).await?;
    txn.commit().await?;

    let status: rpc::AssignStaticAddressStatus = result.into();
    tracing::info!(%interface_id, %ip_address, ?status, "Static address assignment");

    Ok(Response::new(rpc::AssignStaticAddressResponse {
        interface_id: Some(interface_id),
        ip_address: ip_address.to_string(),
        status: status.into(),
    }))
}

pub async fn remove_static_address(
    api: &Api,
    request: Request<rpc::RemoveStaticAddressRequest>,
) -> Result<Response<rpc::RemoveStaticAddressResponse>, CarbideError> {
    let req = request.into_inner();
    let interface_id = req.interface_id.ok_or(CarbideError::InvalidArgument(
        "interface_id is required".into(),
    ))?;
    let ip_address: std::net::IpAddr = req.ip_address.parse()?;

    let mut txn = api.txn_begin().await?;
    let deleted = db::machine_interface_address::delete_by_address(
        &mut txn,
        ip_address,
        AllocationType::Static,
    )
    .await?;
    txn.commit().await?;

    let status = if deleted {
        tracing::info!(%interface_id, %ip_address, "Removed static address");
        rpc::RemoveStaticAddressStatus::Removed
    } else {
        tracing::info!(%interface_id, %ip_address, "Static address not found");
        rpc::RemoveStaticAddressStatus::NotFound
    };

    Ok(Response::new(rpc::RemoveStaticAddressResponse {
        interface_id: Some(interface_id),
        ip_address: ip_address.to_string(),
        status: status.into(),
    }))
}

pub async fn find_interface_addresses(
    api: &Api,
    request: Request<rpc::FindInterfaceAddressesRequest>,
) -> Result<Response<rpc::FindInterfaceAddressesResponse>, Status> {
    let req = request.into_inner();
    let interface_id = req.interface_id.ok_or(CarbideError::InvalidArgument(
        "interface_id is required".into(),
    ))?;

    let mut txn = api.txn_begin().await?;
    let addresses =
        db::machine_interface_address::find_for_interface(&mut txn, interface_id).await?;
    txn.commit().await?;

    let proto_addresses = addresses
        .into_iter()
        .map(|a| rpc::InterfaceAddress {
            address: a.address.to_string(),
            allocation_type: match a.allocation_type {
                AllocationType::Dhcp => "dhcp".to_string(),
                AllocationType::Static => "static".to_string(),
            },
        })
        .collect();

    Ok(Response::new(rpc::FindInterfaceAddressesResponse {
        interface_id: Some(interface_id),
        addresses: proto_addresses,
    }))
}
