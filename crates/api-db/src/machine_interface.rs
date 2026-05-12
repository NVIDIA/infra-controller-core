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

use std::net::IpAddr;
use std::str::FromStr;

use carbide_network::ip::IdentifyAddressFamily;
use carbide_uuid::domain::DomainId;
use carbide_uuid::machine::{MachineId, MachineInterfaceId};
use carbide_uuid::network::{NetworkPrefixId, NetworkSegmentId};
use carbide_uuid::power_shelf::PowerShelfId;
use carbide_uuid::switch::SwitchId;
use chrono::{DateTime, Utc};
use ipnetwork::IpNetwork;
use itertools::Itertools;
use lazy_static::lazy_static;
use mac_address::MacAddress;
use model::address_selection_strategy::AddressSelectionStrategy;
use model::allocation_type::AllocationType;
use model::expected_machine::ExpectedHostNic;
use model::hardware_info::HardwareInfo;
use model::machine::MachineInterfaceSnapshot;
use model::machine_interface::InterfaceType;
use model::machine_interface_address::MachineInterfaceAssociation;
use model::network_prefix::NetworkPrefix;
use model::network_segment::{AllocationStrategy, NetworkSegment, NetworkSegmentType};
use model::predicted_machine_interface::PredictedMachineInterface;
use sqlx::{FromRow, PgConnection, PgTransaction};

use super::{ColumnInfo, FilterableQueryBuilder, ObjectColumnFilter};
use crate::db_read::DbReader;
use crate::ip_allocator::{IpAllocator, UsedIpResolver};
use crate::{DatabaseError, DatabaseResult, Transaction, network_segment as db_network_segment};

const SQL_VIOLATION_DUPLICATE_MAC: &str = "machine_interfaces_segment_id_mac_address_key";
const SQL_VIOLATION_ONE_PRIMARY_INTERFACE: &str = "one_primary_interface_per_machine";
const SQL_VIOLATION_MAX_ONE_ASSOCIATION: &str = "chk_max_one_association";
const FAST_PATH_MAX_RETRIES: usize = 128;
const FAST_PATH_CANDIDATE_BATCH: i64 = 32;

pub struct UsedAdminNetworkIpResolver {
    pub segment_id: NetworkSegmentId,
    // All the IPs which can not be allocated, e.g. SVI IP.
    pub busy_ips: Vec<IpAddr>,
}

#[derive(Clone, Copy)]
pub struct IdColumn;
impl ColumnInfo<'_> for IdColumn {
    type TableType = MachineInterfaceSnapshot;
    type ColumnType = MachineInterfaceId;
    fn column_name(&self) -> &'static str {
        "id"
    }
}

#[derive(Clone, Copy)]
pub struct MacAddressColumn;
impl ColumnInfo<'_> for MacAddressColumn {
    type TableType = MachineInterfaceSnapshot;
    type ColumnType = MacAddress;
    fn column_name(&self) -> &'static str {
        "mac_address"
    }
}

#[derive(Clone, Copy)]
pub struct MachineIdColumn;

impl ColumnInfo<'_> for MachineIdColumn {
    type TableType = MachineInterfaceSnapshot;
    type ColumnType = MachineId;
    fn column_name(&self) -> &'static str {
        "machine_id"
    }
}

#[derive(Clone, Copy)]
pub struct PowerShelfIdColumn;

impl ColumnInfo<'_> for PowerShelfIdColumn {
    type TableType = MachineInterfaceSnapshot;
    type ColumnType = PowerShelfId;
    fn column_name(&self) -> &'static str {
        "power_shelf_id"
    }
}

#[derive(Clone, Copy)]
pub struct SwitchIdColumn;

impl ColumnInfo<'_> for SwitchIdColumn {
    type TableType = MachineInterfaceSnapshot;
    type ColumnType = SwitchId;
    fn column_name(&self) -> &'static str {
        "switch_id"
    }
}

/// A denormalized view on machine_interfaces that aggregates the addresses and vendors using
/// JSON_AGG. This query is also used by machines.rs as a subquery when collecting machine
/// snapshots.
pub const MACHINE_INTERFACE_SNAPSHOT_QUERY: &str = r#"
    SELECT mi.*,
        COALESCE(addresses_agg.json, '[]'::json) AS addresses,
        COALESCE(vendors_agg.json, '[]'::json) AS vendors,
        ns.network_segment_type
    FROM machine_interfaces mi
    JOIN network_segments ns ON ns.id = mi.segment_id
    LEFT JOIN LATERAL (
        SELECT a.interface_id,
            json_agg(a.address) AS json
        FROM machine_interface_addresses a
        WHERE a.interface_id = mi.id
        GROUP BY a.interface_id
    ) AS addresses_agg ON true
    LEFT JOIN LATERAL (
        SELECT d.machine_interface_id,
            json_agg(d.vendor_string) AS json
        FROM dhcp_entries d
        WHERE d.machine_interface_id = mi.id
        GROUP BY d.machine_interface_id
    ) AS vendors_agg ON true
"#;

/// Sets current machine interface primary attribute to provided value.
pub async fn set_primary_interface(
    interface_id: &MachineInterfaceId,
    primary: bool,
    txn: &mut PgConnection,
) -> Result<MachineInterfaceId, DatabaseError> {
    let query = "UPDATE machine_interfaces SET primary_interface=$1 where id=$2::uuid RETURNING id";
    sqlx::query_as(query)
        .bind(primary)
        .bind(*interface_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

pub async fn associate_interface_with_dpu_machine(
    interface_id: &MachineInterfaceId,
    dpu_machine_id: &MachineId,
    txn: &mut PgConnection,
) -> Result<MachineInterfaceId, DatabaseError> {
    let query =
        "UPDATE machine_interfaces SET attached_dpu_machine_id=$1 where id=$2::uuid RETURNING id";
    sqlx::query_as(query)
        .bind(dpu_machine_id)
        .bind(*interface_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

pub async fn associate_bmc_interface_with_machine(
    interface_id: &MachineInterfaceId,
    machine_id: &MachineId,
    txn: &mut PgConnection,
) -> DatabaseResult<MachineInterfaceId> {
    let query = "UPDATE machine_interfaces
        SET machine_id=$1,
            association_type='Machine'::association_type,
            interface_type='Bmc'::interface_type,
            primary_interface=false
        WHERE id=$2::uuid
        RETURNING id";
    sqlx::query_as(query)
        .bind(machine_id)
        .bind(*interface_id)
        .fetch_one(txn)
        .await
        .map_err(|err: sqlx::Error| match err {
            sqlx::Error::Database(e)
                if e.constraint() == Some(SQL_VIOLATION_MAX_ONE_ASSOCIATION) =>
            {
                DatabaseError::MaxOneInterfaceAssociation
            }
            _ => DatabaseError::query(query, err),
        })
}

pub async fn set_interface_type(
    interface_id: &MachineInterfaceId,
    interface_type: InterfaceType,
    txn: &mut PgConnection,
) -> DatabaseResult<MachineInterfaceId> {
    let query = "UPDATE machine_interfaces SET interface_type=$1 WHERE id=$2::uuid RETURNING id";
    sqlx::query_as(query)
        .bind(interface_type)
        .bind(*interface_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

pub async fn associate_interface_with_machine(
    interface_id: &MachineInterfaceId,
    association: MachineInterfaceAssociation,
    txn: &mut PgConnection,
) -> DatabaseResult<MachineInterfaceId> {
    let (column_name, association_type, id_value) = match association {
        MachineInterfaceAssociation::Machine(id) => ("machine_id", "Machine", id.to_string()),
        MachineInterfaceAssociation::Switch(id) => ("switch_id", "Switch", id.to_string()),
        MachineInterfaceAssociation::PowerShelf(id) => {
            ("power_shelf_id", "PowerShelf", id.to_string())
        }
    };
    let query = format!(
        "UPDATE machine_interfaces SET {}=$1, association_type=$2::association_type where id=$3::uuid RETURNING id",
        column_name
    );
    sqlx::query_as(&query)
        .bind(id_value)
        .bind(association_type)
        .bind(*interface_id)
        .fetch_one(txn)
        .await
        .map_err(|err: sqlx::Error| match err {
            sqlx::Error::Database(e)
                if e.constraint() == Some(SQL_VIOLATION_ONE_PRIMARY_INTERFACE) =>
            {
                DatabaseError::OnePrimaryInterface
            }
            sqlx::Error::Database(e)
                if e.constraint() == Some(SQL_VIOLATION_MAX_ONE_ASSOCIATION) =>
            {
                DatabaseError::MaxOneInterfaceAssociation
            }
            _ => DatabaseError::query(&query, err),
        })
}

pub async fn find_by_mac_address(
    txn: impl DbReader<'_>,
    macaddr: MacAddress,
) -> Result<Vec<MachineInterfaceSnapshot>, DatabaseError> {
    find_by(txn, ObjectColumnFilter::One(MacAddressColumn, &macaddr)).await
}

pub async fn find_by_ip(
    txn: impl DbReader<'_>,
    ip: IpAddr,
) -> Result<Option<MachineInterfaceSnapshot>, DatabaseError> {
    lazy_static! {
        static ref query: String = format!(
            r#"{}
            INNER JOIN machine_interface_addresses mia on mia.interface_id=mi.id
            WHERE mia.address = $1::inet"#,
            MACHINE_INTERFACE_SNAPSHOT_QUERY
        );
    }
    sqlx::query_as(&query)
        .bind(ip)
        .fetch_optional(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))
}

pub async fn find_all(txn: &mut PgConnection) -> DatabaseResult<Vec<MachineInterfaceSnapshot>> {
    find_by(txn, ObjectColumnFilter::All::<IdColumn>).await
}

pub async fn find_by_machine_ids(
    txn: &mut PgConnection,
    machine_ids: &[MachineId],
) -> Result<std::collections::HashMap<MachineId, Vec<MachineInterfaceSnapshot>>, DatabaseError> {
    use itertools::Itertools;
    // The .unwrap() in the `group_map_by` call is ok - because we are only
    // searching for Machines which have associated MachineIds
    Ok(
        find_by(txn, ObjectColumnFilter::List(MachineIdColumn, machine_ids))
            .await?
            .into_iter()
            .filter(|interface| interface.interface_type != InterfaceType::Bmc)
            .into_group_map_by(|interface| interface.machine_id.unwrap()),
    )
}

pub async fn count_by_segment_id(
    txn: &mut PgConnection,
    segment_id: &NetworkSegmentId,
) -> Result<usize, DatabaseError> {
    let query = "SELECT count(*) FROM machine_interfaces WHERE segment_id = $1";
    let (address_count,): (i64,) = sqlx::query_as(query)
        .bind(segment_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;

    Ok(address_count.max(0) as usize)
}

pub async fn find_one(
    txn: impl DbReader<'_>,
    interface_id: MachineInterfaceId,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    let mut interfaces = find_by(txn, ObjectColumnFilter::One(IdColumn, &interface_id)).await?;
    match interfaces.len() {
        0 => Err(DatabaseError::FindOneReturnedNoResultsError(
            interface_id.into(),
        )),
        1 => Ok(interfaces.remove(0)),
        _ => Err(DatabaseError::FindOneReturnedManyResultsError(
            interface_id.into(),
        )),
    }
}

// Returns (MachineInterface, newly_created_interface).
// newly_created_interface indicates that we couldn't find a
// MachineInterface, so created new one.
//
// `is_primary` integrates `ExpectedHostNic.primary` into machine
// interface creation. If True, this NIC is declared the primary
// boot NIC (which is/was the previous default behavior anyway,
// meaning None does the same thing), and this is fine, because
// at the end of the day, site-explorer will end up demoting it
// as part of attaching a DPU.
//
// Now, if it's *False*, there's a different NIC on this host declared
// as the boot NIC, so we actually overide the new interface and
// explicitly mark it as non-primary here. We *could* bake this in
// as part of validate_existing_mac_and_create, but since this is
// the only call-site that cares about it, I'm making it specific
// to here.
// TODO(chet): ...but consider plumbing it through.
//
// If we're not making a new interface, then existing interfaces
// are returned untouched.
pub async fn find_or_create_machine_interface(
    txn: &mut PgConnection,
    machine_id: Option<MachineId>,
    mac_address: MacAddress,
    relays: &[IpAddr],
    host_nic: Option<ExpectedHostNic>,
    is_primary: Option<bool>,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    let relaystr = relays
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<String>>()
        .join(", ");
    match machine_id {
        None => {
            tracing::info!(
                %mac_address,
                "Found no existing machine with mac address {mac_address} using networks with relays {relaystr}",
            );
            // validate_existing_mac_and_create hardcodes primary_interface: true
            // at creation. If the caller has explicitly declared a *different*
            // NIC as this machine's primary (i.e. is_primary == false), override the
            // true/default here.
            let mut interface =
                validate_existing_mac_and_create(&mut *txn, mac_address, relays, host_nic).await?;
            if is_primary == Some(false) && interface.primary_interface {
                set_primary_interface(&interface.id, false, &mut *txn).await?;
                interface.primary_interface = false;
            }
            Ok(interface)
        }
        Some(_) => {
            let mut ifcs = find_by_mac_address(&mut *txn, mac_address).await?;
            match ifcs.len() {
                1 => Ok(ifcs.remove(0)),
                n => {
                    tracing::warn!(
                        %mac_address,
                        relay_ips = %relaystr,
                        num_mac_address = n,
                        "Duplicate mac address for network segment",
                    );
                    Err(DatabaseError::NetworkSegmentDuplicateMacAddress(
                        mac_address,
                    ))
                }
            }
        }
    }
}

/// Do basic validating on existing macs and create the interface if it does not exist
pub async fn validate_existing_mac_and_create(
    txn: &mut PgConnection,
    mac_address: MacAddress,
    relays: &[IpAddr],
    host_nic: Option<ExpectedHostNic>,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    let mut interface_snapshot = find_by_mac_address(&mut *txn, mac_address).await?;
    match &interface_snapshot.len() {
        0 => {
            tracing::debug!(
                %mac_address,
                "No existing machine_interface with mac address exists yet, creating one",
            );

            let segment_type = if let Some(nic) = host_nic.clone() {
                if let Some(nic_type) = nic.nic_type {
                    match nic_type.to_ascii_lowercase().as_str() {
                        "bf3" => Some(NetworkSegmentType::Admin),
                        "dpu" => Some(NetworkSegmentType::Admin),
                        "bmc" => Some(NetworkSegmentType::Underlay),
                        "oob" => Some(NetworkSegmentType::Underlay),
                        "onboard" => Some(NetworkSegmentType::Admin),
                        &_ => None, // (default) use the relay ip if not forcing a segment type
                    }
                } else {
                    None
                }
            } else {
                None
            };

            let network_segments = if let Some(network_segment_type) = segment_type {
                // only if forcing a segment type
                db_network_segment::for_segment_type_all(txn, relays, network_segment_type).await?
            } else {
                db_network_segment::for_relay_all(txn, relays).await?
            };

            if !network_segments.is_empty() {
                // If the segment only allows static reservations, reject
                // dynamic allocation. The device must have a pre-existing
                // static reservation to get an IP on this segment.
                for segment in network_segments.iter() {
                    if segment.allocation_strategy == AllocationStrategy::Reserved {
                        return Err(DatabaseError::internal(format!(
                            "segment {} configured for static DHCP leases only; no static reservation for MAC {mac_address}",
                            segment.name,
                        )));
                    }
                }

                // If a fixed IP is specified for this NIC, use static
                // allocation instead of the pool allocator.
                let strategy = if let Some(ref expected_nic) = host_nic
                    && let Some(ref ipaddr) = expected_nic.fixed_ip
                {
                    let fixed_addr: IpAddr = ipaddr.parse().map_err(|_| {
                        DatabaseError::internal(format!(
                            "invalid fixed_ip '{ipaddr}' for MAC {mac_address}"
                        ))
                    })?;
                    AddressSelectionStrategy::StaticAddress(fixed_addr)
                } else {
                    AddressSelectionStrategy::NextAvailableIp
                };

                let v = create(txn, &network_segments, &mac_address, true, strategy).await?;
                Ok(v)
            } else {
                Err(DatabaseError::internal(format!(
                    "No network segment defined for relay addresses: {:?}",
                    relays
                )))
            }
        }
        1 => {
            tracing::debug!(
                %mac_address,
                "Mac address exists, validating the relay and returning it",
            );

            // TODO(chet): I don't like that it's mut here, but this seems to be
            // a pattern in this module in general, especially since we may or may
            // not update the interface. Consider having reconcile_interface_segment
            // just return the interface, which would probably look a lot better.
            let mut existing_interface = interface_snapshot.remove(0);
            reconcile_interface_segment(txn, &mut existing_interface, relays).await?;
            Ok(existing_interface)
        }
        _ => {
            tracing::warn!(
                %mac_address,
                // A MAC address should identify a single machine interface within a segment.
                "More than one existing mac address for network segment",
            );
            Err(DatabaseError::NetworkSegmentDuplicateMacAddress(
                mac_address,
            ))
        }
    }
}

/// Ensure a a `machine_interface` exists for the `mac_address` with its reserved allocation,
/// either falling into a Carbide-managed segment (when there is a match within a managed
/// prefix), or into the `static_assignments` segment for IPs that are outside of managed
/// networks.
///
/// Errors on conflicts that need operator attention, e.g.
/// - The interface for this MAC exists but carries different addresses, or,
/// - The IP is already allocated to a different MAC.
///
/// Called from the expected-machine handlers to do config time preallocation, and from
/// the DHCP `discover()` path to re-allocate if needed (ensuring static DHCP reservations
/// are maintained). Tbh it could probably just be in the DHCP `discover()` path only.
pub async fn preallocate_machine_interface(
    txn: &mut PgConnection,
    mac_address: MacAddress,
    static_ip: IpAddr,
) -> DatabaseResult<()> {
    preallocate_machine_interface_with_type(txn, mac_address, static_ip, InterfaceType::Data).await
}

pub async fn preallocate_bmc_machine_interface(
    txn: &mut PgConnection,
    mac_address: MacAddress,
    static_ip: IpAddr,
) -> DatabaseResult<()> {
    preallocate_machine_interface_with_type(txn, mac_address, static_ip, InterfaceType::Bmc).await
}

async fn preallocate_machine_interface_with_type(
    txn: &mut PgConnection,
    mac_address: MacAddress,
    static_ip: IpAddr,
    interface_type: InterfaceType,
) -> DatabaseResult<()> {
    let existing = find_by_mac_address(&mut *txn, mac_address).await?;
    if let Some(iface) = existing.first() {
        if iface.addresses.contains(&static_ip) {
            if iface.interface_type != interface_type {
                set_interface_type(&iface.id, interface_type, txn).await?;
            }
            if interface_type == InterfaceType::Bmc && iface.primary_interface {
                set_primary_interface(&iface.id, false, txn).await?;
            }
            return Ok(());
        }
        return Err(DatabaseError::InvalidArgument(format!(
            "a machine interface already exists for MAC {mac_address} with addresses {:?}; use update to change the IP address",
            iface.addresses,
        )));
    }

    if let Some(existing_addr) =
        crate::machine_interface_address::find_by_address(&mut *txn, static_ip).await?
    {
        return Err(DatabaseError::InvalidArgument(format!(
            "IP address {static_ip} is already allocated to interface {} on segment {}; use 'machine-interfaces assign-address' to reassign it",
            existing_addr.id, existing_addr.name,
        )));
    }

    let segment = match db_network_segment::for_relay(&mut *txn, static_ip).await? {
        Some(seg) => seg,
        None => db_network_segment::static_assignments(&mut *txn).await?,
    };

    create_with_type(
        txn,
        std::slice::from_ref(&segment),
        &mac_address,
        interface_type != InterfaceType::Bmc,
        AddressSelectionStrategy::StaticAddress(static_ip),
        interface_type,
    )
    .await?;

    tracing::info!(
        %mac_address,
        %static_ip,
        segment_id = %segment.id,
        "Pre-allocated static machine interface"
    );

    Ok(())
}

pub async fn create(
    txn: &mut PgConnection,
    segments: &[NetworkSegment],
    macaddr: &MacAddress,
    primary_interface: bool,
    address_strategy: AddressSelectionStrategy,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    create_with_type(
        txn,
        segments,
        macaddr,
        primary_interface,
        address_strategy,
        InterfaceType::Data,
    )
    .await
}

pub async fn create_with_type(
    txn: &mut PgConnection,
    segments: &[NetworkSegment],
    macaddr: &MacAddress,
    primary_interface: bool,
    address_strategy: AddressSelectionStrategy,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    match address_strategy {
        AddressSelectionStrategy::NextAvailableIp | AddressSelectionStrategy::Automatic => {
            create_fast_path(txn, segments, macaddr, primary_interface, interface_type).await
        }
        AddressSelectionStrategy::StaticAddress(addr) => {
            create_static_path(
                txn,
                segments,
                macaddr,
                primary_interface,
                addr,
                interface_type,
            )
            .await
        }
        //
        AddressSelectionStrategy::NextAvailablePrefix(_) => {
            let [segment] = segments else {
                return Err(DatabaseError::InvalidArgument(
                    "NextAvailablePrefix allocation requires exactly one candidate segment"
                        .to_string(),
                ));
            };

            create_slow_path(
                txn,
                segment,
                macaddr,
                primary_interface,
                address_strategy,
                interface_type,
            )
            .await
        }
    }
}

#[allow(txn_held_across_await)]
async fn create_fast_path(
    txn: &mut PgConnection,
    segments: &[NetworkSegment],
    macaddr: &MacAddress,
    primary_interface: bool,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    for segments_idx in 0..segments.len() {
        let segment = &segments[segments_idx];
        for _ in 0..FAST_PATH_MAX_RETRIES {
            let mut fast_txn = Transaction::begin_inner(txn).await?;

            // Make sure we're mutually exclusive with the slow path: a shared lock means many fast-path
            // allocations can happen concurrently, but the slow path will hold this exclusively
            // (waiting on any shared locks to complete)
            lock_network_segment_shared(&mut fast_txn, segment).await?;

            let segment_exhausted = match try_create_fast_path(
                &mut fast_txn,
                segment,
                macaddr,
                primary_interface,
                interface_type,
            )
            .await
            {
                Ok(interface_id) => {
                    fast_txn.commit().await?;
                    return Ok(
                        find_by(txn, ObjectColumnFilter::One(IdColumn, &interface_id))
                            .await?
                            .remove(0),
                    );
                }
                Err(err) if err.is_fqdn_conflict() => {
                    // Another simultaneous create got the same FQDN, try again.
                    false
                }
                Err(DatabaseError::TryAgain) => {
                    // All the IP's in the batch we grabbed from the database got taken by other
                    // concurrent calls to create_fast_path. Try again.
                    false
                }
                Err(DatabaseError::ResourceExhausted(_)) if segments_idx < segments.len() - 1 => {
                    // If there are more segments to check, we just need to signal that this one was exhausted.
                    true
                }
                Err(err) => {
                    // Some other error, roll back the inner transaction
                    fast_txn.rollback().await?;
                    return Err(err);
                }
            };

            fast_txn.rollback().await?;
            tokio::task::yield_now().await;

            // If this segment is exhausted, go to the next segment.
            if segment_exhausted {
                break;
            }
        }
    }

    Err(DatabaseError::internal(format!(
        "unable to create machine interface in fast path out of segments {:?} after {} retries",
        segments, FAST_PATH_MAX_RETRIES
    )))
}

/// Create a machine interface with a specific static IP address.
/// A perfect compliment to create_fast_path and create_slow_path.
async fn create_static_path(
    txn: &mut PgConnection,
    segments: &[NetworkSegment],
    macaddr: &MacAddress,
    primary_interface: bool,
    address: IpAddr,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    // For the staic path, we need to be a little forgiving since
    // we expect to allow static assignment even if the requested
    // assignment is outside any network segment as long as
    // there is a "static assignment segment".
    // To identify the owning segment:
    //  - pick a segment whose prefix contains the static IP (we guard against overlap so there could be at most 1)
    //  - otherwise allow the special static-assignments segment
    //  - otherwise return an error
    let segment = segments
                .iter()
                .find(|s| s.prefixes.iter().any(|p| p.prefix.contains(address)))
                .or_else(|| segments.iter().find(|s| s.name == crate::network_segment::STATIC_ASSIGNMENTS_SEGMENT_NAME))
                .ok_or_else(|| DatabaseError::internal(format!(
                    "unable to find network segment that contains requested IP {address} in network segments: {}",
                    segments.iter().map(|s| s.id.to_string()).join(", "),
                ))
    )?;

    let interface_id = create_inner(
        txn,
        segment,
        macaddr,
        segment.subdomain_id,
        primary_interface,
        &[address],
        AllocationType::Static,
        interface_type,
    )
    .await?;

    Ok(
        find_by(txn, ObjectColumnFilter::One(IdColumn, &interface_id))
            .await?
            .remove(0),
    )
}

/// Create a machine interface and allocate IP addresses, slow path for whole-prefix allocation.
///
/// This uses [`crate::IpAllocator`], which requires:
///
/// - Locking the machine_interfaces_lock table
/// - Reading all used IP's from the database for the given segment
/// - Selecting a batch of IP's according to the selection strategy
#[allow(txn_held_across_await)]
pub async fn create_slow_path(
    txn: &mut PgConnection,
    segment: &NetworkSegment,
    macaddr: &MacAddress,
    primary_interface: bool,
    address_strategy: AddressSelectionStrategy,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    // We're potentially about to insert a couple rows, so create a savepoint.
    let mut inner_txn = Transaction::begin_inner(txn).await?;

    // If either requested addresses are auto-generated, we lock the entire table
    // by way of the inner_txn.
    lock_network_segment_exclusive(&mut inner_txn, segment).await?;

    // Collect SVI IPs so the allocator knows they're already reserved.
    let mut reserved_ips = vec![];
    for prefix in &segment.prefixes {
        if let Some(svi_ip) = prefix.svi_ip {
            reserved_ips.push(svi_ip);
        }
    }

    let dhcp_handler: Box<dyn UsedIpResolver<PgConnection> + Send> =
        Box::new(UsedAdminNetworkIpResolver {
            segment_id: segment.id,
            busy_ips: reserved_ips,
        });

    // Allocate an address from each prefix in the segment. For dual-stack
    // segments this means one IPv4 address and one IPv6 address.
    let allocator = IpAllocator::new(
        inner_txn.as_pgconn(),
        segment,
        dhcp_handler,
        address_strategy,
    )
    .await?;

    let mut allocated_addresses = Vec::new();
    for (_, maybe_address) in allocator {
        let address = maybe_address?;
        allocated_addresses.push(address.ip());
    }

    let interface_id = create_inner(
        inner_txn.as_pgconn(),
        segment,
        macaddr,
        segment.subdomain_id,
        primary_interface,
        &allocated_addresses,
        AllocationType::Dhcp,
        interface_type,
    )
    .await?;
    inner_txn.commit().await?;

    Ok(
        find_by(txn, ObjectColumnFilter::One(IdColumn, &interface_id))
            .await?
            .remove(0),
    )
}

/// Fast path for single-IP allocation.
///
/// This allocates a single candidate IP per prefix entirely in the database, without having to read
/// all the used IP's.
async fn try_create_fast_path(
    // Note: Must be a transaction since we're doing locks
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
    macaddr: &MacAddress,
    primary_interface: bool,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceId> {
    let allocated_addresses = allocate_addresses_from_segment(txn, segment).await?;

    create_inner(
        txn,
        segment,
        macaddr,
        segment.subdomain_id,
        primary_interface,
        &allocated_addresses,
        AllocationType::Dhcp,
        interface_type,
    )
    .await
}

/// Allocate one IP address from each prefix in the segment.
/// For dual-stack segments this means one IPv4 and one IPv6 address.
async fn allocate_addresses_from_segment(
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
) -> DatabaseResult<Vec<IpAddr>> {
    let mut addresses = Vec::with_capacity(segment.prefixes.len());
    for prefix in &segment.prefixes {
        let address = allocate_next_ip_with_retry(txn, segment, prefix).await?;
        addresses.push(address);
    }
    Ok(addresses)
}

/// Create the actual machine interface once we know what addresses we want.
#[allow(clippy::too_many_arguments)]
async fn create_inner(
    txn: &mut PgConnection,
    segment: &NetworkSegment,
    macaddr: &MacAddress,
    domain_id: Option<DomainId>,
    primary_interface: bool,
    allocated_addresses: &[IpAddr],
    allocation_type: AllocationType,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceId> {
    // Prefer IPv4 for hostname (more human-readable), fall back to
    // an IPv6-derived hostname otherwise.
    let hostname_address = allocated_addresses
        .iter()
        .find(|a| a.is_ipv4())
        .or(allocated_addresses.first())
        .ok_or_else(|| {
            let prefixes: Vec<_> = segment
                .prefixes
                .iter()
                .map(|p| p.prefix.to_string())
                .collect();
            crate::DatabaseError::ResourceExhausted(format!(
                "No IP addresses left in network segment (prefixes: {})",
                prefixes.join(", ")
            ))
        })?;
    let hostname = address_to_hostname(hostname_address)?;

    let interface_id = insert_machine_interface(
        txn,
        &segment.id,
        macaddr,
        hostname,
        domain_id,
        primary_interface,
        interface_type,
    )
    .await?;

    for address in allocated_addresses {
        insert_machine_interface_address(txn, &interface_id, address, allocation_type).await?;
    }

    Ok(interface_id)
}

/// Retries allocation for a single prefix which may be under contention.
///
/// Each iteration fetches a small free-IP batch, tries to take an advisory lock
/// on each candidate, and returns once one lock is acquired.
///
/// This is for eliminating a big shared lock when we have lots of machines DHCP'ing for the first
/// time simultaneously: By requesting a batch of free IP's at once and trying locks on each one, we
/// can process roughly [`FAST_PATH_CANDIDATE_BATCH`] initial DHCP requests concurrently.
async fn allocate_next_ip_with_retry(
    // Note: Must be a transaction since we're doing locks
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
    prefix: &NetworkPrefix,
) -> DatabaseResult<IpAddr> {
    let reserved = if prefix.gateway.is_none() {
        prefix.num_reserved.max(2)
    } else {
        prefix.num_reserved.max(1)
    };

    let network_bit_width = match prefix.prefix {
        IpNetwork::V4(_) => 32,
        IpNetwork::V6(_) => 128,
    };

    for _ in 0..FAST_PATH_MAX_RETRIES {
        // Grab FAST_PATH_CANDIDATE_BATCH IP's at once
        let query = r#"
SELECT ($1::inet + ip_series.n)::inet AS ip
FROM generate_series($3, (1 << $2) - 2) AS ip_series(n)
LEFT JOIN machine_interface_addresses AS mia
  ON mia.address = ($1::inet + ip_series.n)::inet
WHERE mia.address IS NULL
  AND ($4::inet IS NULL OR ($1::inet + ip_series.n)::inet <> $4::inet)
  AND ($5::inet IS NULL OR ($1::inet + ip_series.n)::inet <> $5::inet)
ORDER BY ip
LIMIT $6;
    "#;
        let candidates = sqlx::query_scalar::<_, IpAddr>(query)
            .bind(prefix.prefix.ip())
            .bind(network_bit_width - prefix.prefix.prefix() as i32)
            .bind(reserved)
            .bind(prefix.gateway)
            .bind(prefix.svi_ip)
            .bind(FAST_PATH_CANDIDATE_BATCH)
            .fetch_all(txn.as_mut())
            .await
            .map_err(|e| DatabaseError::query(query, e))?;

        if candidates.is_empty() {
            return Err(DatabaseError::ResourceExhausted(format!(
                "No IP addresses left in prefix {}",
                prefix.prefix
            )));
        }

        // Try to lock an IP (in case multiple allocation requests are happening at once)
        for candidate in candidates {
            if try_lock_ip_candidate(txn, segment, candidate).await? {
                return Ok(candidate);
            }
        }
    }

    Err(DatabaseError::TryAgain)
}

/// Attempts to acquire a transaction-scoped advisory lock for one IP candidate.
///
/// A successful lock means this transaction "owns" that candidate for the current attempt, which
/// avoids same-IP races across concurrent allocations.
async fn try_lock_ip_candidate(
    // Note: Must be a transaction since we're doing locks
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
    ip: IpAddr,
) -> DatabaseResult<bool> {
    let query = "SELECT pg_try_advisory_xact_lock(hashtextextended($1::text, 0))";
    sqlx::query_scalar::<_, bool>(query)
        .bind(format!("{}:{}", segment.id, ip))
        .fetch_one(txn.as_mut())
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

async fn lock_network_segment_shared(
    // Note: Must be a transaction since we're doing locks
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
) -> DatabaseResult<()> {
    let query = "SELECT pg_advisory_xact_lock_shared(hashtextextended($1::text, 0))";
    sqlx::query_scalar(query)
        .bind(format!("network_segment.{}", segment.id))
        .fetch_one(txn.as_mut())
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

async fn lock_network_segment_exclusive(
    // Note: Must be a transaction since we're doing locks
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
) -> DatabaseResult<()> {
    let query = "SELECT pg_advisory_xact_lock(hashtextextended($1::text, 0))";
    sqlx::query_scalar(query)
        .bind(format!("network_segment.{}", segment.id))
        .fetch_one(txn.as_mut())
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

pub async fn allocate_svi_ip(
    txn: &mut PgTransaction<'_>,
    segment: &NetworkSegment,
) -> DatabaseResult<(NetworkPrefixId, IpAddr)> {
    let dhcp_handler: Box<dyn UsedIpResolver<PgConnection> + Send> =
        Box::new(UsedAdminNetworkIpResolver {
            segment_id: segment.id,
            busy_ips: vec![],
        });

    // Prevent other allocations from happening concurrently in this network segment
    lock_network_segment_exclusive(txn, segment).await?;

    let mut addresses_allocator = IpAllocator::new(
        txn.as_mut(),
        segment,
        dhcp_handler,
        AddressSelectionStrategy::NextAvailableIp,
    )
    .await?;
    match addresses_allocator.next() {
        Some((id, Ok(address))) => Ok((id, address.ip())),
        Some((_, Err(err))) => Err(err),
        _ => Err(DatabaseError::ResourceExhausted(format!(
            "SVI IP not found for {}.",
            segment.id
        ))),
    }
}

// Support dpu-agent/scout transition from machine_interface_id to source IP.
// Allow either for now.
pub async fn find_by_ip_or_id(
    txn: &mut PgConnection,
    remote_ip: Option<IpAddr>,
    interface_id: Option<MachineInterfaceId>,
) -> Result<MachineInterfaceSnapshot, DatabaseError> {
    if let Some(remote_ip) = remote_ip
        && let Some(interface) = find_by_ip(&mut *txn, remote_ip).await?
    {
        // remove debug message by Apr 2024
        tracing::debug!(
            interface_id = %interface.id,
            %remote_ip,
            "Loaded interface by remote IP"
        );
        return Ok(interface);
    }
    match interface_id {
        Some(interface_id) => find_one(txn, interface_id).await,
        None => Err(DatabaseError::NotFoundError {
            kind: "machine_interface",
            id: format!("remote_ip={remote_ip:?},interface_id={interface_id:?}"),
        }),
    }
}

/// insert_machine_interface inserts a new machine interface record
/// into the database, returning the newly minted MachineInterfaceId
/// for the corresponding record.
async fn insert_machine_interface(
    txn: &mut PgConnection,
    segment_id: &NetworkSegmentId,
    mac_address: &MacAddress,
    hostname: String,
    domain_id: Option<DomainId>,
    is_primary_interface: bool,
    interface_type: InterfaceType,
) -> DatabaseResult<MachineInterfaceId> {
    let query = "INSERT INTO machine_interfaces
        (segment_id, mac_address, hostname, domain_id, primary_interface, interface_type)
        VALUES
        ($1::uuid, $2::macaddr, $3::varchar, $4::uuid, $5::bool, $6::interface_type) RETURNING id";

    let (interface_id,): (MachineInterfaceId,) = sqlx::query_as(query)
        .bind(segment_id)
        .bind(mac_address)
        .bind(hostname)
        .bind(domain_id)
        .bind(is_primary_interface)
        .bind(interface_type)
        .fetch_one(txn)
        .await
        .map_err(|err: sqlx::Error| match err {
            sqlx::Error::Database(e) if e.constraint() == Some(SQL_VIOLATION_DUPLICATE_MAC) => {
                DatabaseError::NetworkSegmentDuplicateMacAddress(*mac_address)
            }
            sqlx::Error::Database(e)
                if e.constraint() == Some(SQL_VIOLATION_ONE_PRIMARY_INTERFACE) =>
            {
                DatabaseError::OnePrimaryInterface
            }
            _ => DatabaseError::query(query, err),
        })?;

    Ok(interface_id)
}

/// insert_machine_interface_address inserts a new machine interface
/// address entry into the database. In the case of machine interfaces,
/// this explicitly takes an `IpAddr`, since machine interfaces are
/// always going to be a /32. It is up to the caller to ensure a possible
/// IpNetwork returned from the IpAllocator is of the correct size.
async fn insert_machine_interface_address(
    txn: &mut PgConnection,
    interface_id: &MachineInterfaceId,
    address: &IpAddr,
    allocation_type: model::allocation_type::AllocationType,
) -> DatabaseResult<()> {
    let query = "INSERT INTO machine_interface_addresses (interface_id, address, allocation_type) VALUES ($1::uuid, $2::inet, $3)";
    sqlx::query(query)
        .bind(interface_id)
        .bind(address)
        .bind(allocation_type)
        .execute(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(())
}

/// address_to_hostname converts an IpAddr address to a hostname,
/// verifying the resulting hostname is actually a valid DNS name
/// before returning it.
///
/// IPv4: replaces dots with dashes, e.g. `192.168.1.2` → `192-168-1-2`
/// IPv6: expands to full form and replaces colons with dashes,
///       e.g. `2001:db8::2` → `2001-0db8-0000-0000-0000-0000-0000-0002`
fn address_to_hostname(address: &IpAddr) -> DatabaseResult<String> {
    let hostname = match address {
        IpAddr::V4(_) => address.to_string().replace('.', "-"),
        IpAddr::V6(v6) => v6
            .segments()
            .iter()
            .map(|s| format!("{s:04x}"))
            .collect::<Vec<_>>()
            .join("-"),
    };
    match domain::base::Name::<octseq::array::Array<255>>::from_str(hostname.as_str()).is_ok() {
        true => Ok(hostname),
        false => Err(DatabaseError::internal(format!(
            "invalid address to hostname: {hostname}"
        ))),
    }
}

async fn find_by<'a, C: ColumnInfo<'a, TableType = MachineInterfaceSnapshot>>(
    txn: impl DbReader<'_>,
    filter: ObjectColumnFilter<'a, C>,
) -> Result<Vec<MachineInterfaceSnapshot>, DatabaseError> {
    let mut query = FilterableQueryBuilder::new(MACHINE_INTERFACE_SNAPSHOT_QUERY)
        .filter_relation(&filter, Some("mi"));
    let interfaces = query
        .build_query_as::<MachineInterfaceSnapshot>()
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(query.sql(), e))?;
    Ok(interfaces)
}

pub async fn get_machine_interface_primary(
    machine_id: &MachineId,
    txn: &mut PgConnection,
) -> DatabaseResult<MachineInterfaceSnapshot> {
    find_by_machine_ids(txn, &[*machine_id])
        .await?
        .remove(machine_id)
        .ok_or_else(|| DatabaseError::NotFoundError {
            kind: "interface",
            id: machine_id.to_string(),
        })?
        .into_iter()
        .filter(|m_intf| m_intf.primary_interface)
        .collect::<Vec<MachineInterfaceSnapshot>>()
        .pop()
        .ok_or_else(|| {
            DatabaseError::internal(format!("Couldn't find primary interface for {machine_id}."))
        })
}

/// Move an entry from predicted_machine_interfaces to machine_interfaces, using the given relay IP
/// to know what network segment to assign.
pub async fn move_predicted_machine_interface_to_machine(
    txn: &mut PgConnection,
    predicted_machine_interface: &PredictedMachineInterface,
    relay_ip: IpAddr,
) -> Result<(), DatabaseError> {
    tracing::info!(
        machine_id=%predicted_machine_interface.machine_id,
        mac_address=%predicted_machine_interface.mac_address,
        %relay_ip,
        "Got DHCP from predicted machine interface, moving to machine"
    );
    let Some(network_segment) = crate::network_segment::for_relay(txn, relay_ip).await? else {
        return Err(DatabaseError::internal(format!(
            "No network segment defined for relay address: {relay_ip}"
        )));
    };

    if network_segment.segment_type != predicted_machine_interface.expected_network_segment_type {
        return Err(DatabaseError::internal(format!(
            "Got DHCP for predicted host with MAC address {0} on network segment {1}, which is not of the expected type {2}",
            predicted_machine_interface.mac_address,
            network_segment.id,
            predicted_machine_interface.expected_network_segment_type,
        )));
    }

    let machine_interface_id = match self::find_by_mac_address(
        &mut *txn,
        predicted_machine_interface.mac_address,
    )
    .await?
    .into_iter()
    .find(|machine_interface| machine_interface.segment_id == network_segment.id)
    {
        Some(machine_interface_snapshot) => {
            match machine_interface_snapshot.machine_id.as_ref() {
                None => {
                    // This host has already DHCP'd once and created an anonymous machine_interface,
                    // we will migrate it below.
                    machine_interface_snapshot.id
                }
                Some(machine_id) => {
                    if machine_id.ne(&predicted_machine_interface.machine_id) {
                        tracing::error!(
                            %machine_id,
                            "Can't migrate predicted_machine_interface to machine_interface: one already exists with this MAC address"
                        );
                        return Err(DatabaseError::NetworkSegmentDuplicateMacAddress(
                            predicted_machine_interface.mac_address,
                        ));
                    } else {
                        tracing::warn!(
                            %machine_id,
                            "Bug: trying to move predicted_machine_interface to machine_interface, but it's already a part of this machine? Will proceed anyway."
                        );
                        machine_interface_snapshot.id
                    }
                }
            }
        }
        None => {
            // This host has never DHCP'd before, create a new machine_interface for it
            let machine_interface = create(
                txn,
                &[network_segment],
                &predicted_machine_interface.mac_address,
                false,
                AddressSelectionStrategy::NextAvailableIp,
            )
            .await?;
            machine_interface.id
        }
    };

    // Take either the newly-created interface or the anonymous one we found, and associate it with
    // this machine.
    associate_interface_with_machine(
        &machine_interface_id,
        MachineInterfaceAssociation::Machine(predicted_machine_interface.machine_id),
        txn,
    )
    .await?;

    crate::predicted_machine_interface::delete(predicted_machine_interface, txn).await?;
    Ok(())
}

/// This function creates Proactive Host Machine Interface with all available information.
/// Parsed Mac: Found in DPU's topology data
/// Relay IP: Taken from fixed Admin network segment. Relay IP is used only to identify related
/// segment.
/// Returns: Machine Interface, True if new interface is created.
pub async fn create_host_machine_dpu_interface_proactively(
    txn: &mut PgConnection,
    hardware_info: Option<&HardwareInfo>,
    dpu_id: &MachineId,
) -> Result<MachineInterfaceSnapshot, DatabaseError> {
    let admin_networks = crate::network_segment::admin(txn).await?;

    // Using gateway IP as relay IP. This is just to enable next algorithm to find related network
    // segment.
    let mut gateways = vec![];
    let mut existing_machine = None;

    for admin_network in admin_networks {
        for prefix in admin_network.prefixes {
            if let Some(gateway) = prefix.gateway {
                gateways.push(gateway);
            }
        }
    }

    if gateways.is_empty() {
        return Err(DatabaseError::AdminNetworkNotConfigured);
    };

    // Host mac is stored at DPU topology data.
    let host_mac = hardware_info
        .map(|x| x.factory_mac_address())
        .ok_or_else(|| DatabaseError::NotFoundError {
            kind: "Hardware Info",
            id: dpu_id.to_string(),
        })??;

    for gateway in gateways.iter() {
        existing_machine =
            crate::machine::find_existing_machine(txn, host_mac, gateway.to_owned()).await?;
        if existing_machine.is_some() {
            break;
        }
    }

    let machine_interface =
        find_or_create_machine_interface(txn, existing_machine, host_mac, &gateways, None, None)
            .await?;
    associate_interface_with_dpu_machine(&machine_interface.id, dpu_id, txn).await?;

    Ok(machine_interface)
}

pub async fn find_by_machine_and_segment(
    txn: &mut PgConnection,
    machine_id: &MachineId,
    segment_id: NetworkSegmentId,
) -> Result<Vec<MachineInterfaceSnapshot>, DatabaseError> {
    lazy_static! {
        static ref query: String = format!(
            "{} WHERE mi.machine_id = $1 AND mi.segment_id = $2::uuid",
            MACHINE_INTERFACE_SNAPSHOT_QUERY
        );
    }
    sqlx::query_as::<_, MachineInterfaceSnapshot>(&query)
        .bind(machine_id)
        .bind(segment_id)
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))
        .map(|interfaces| interfaces.into_iter().collect())
}

/// Update the segment_id and domain_id for a machine interface. Used
/// when a static address assignment or DHCP re-discovery places an
/// interface on a different segment than it was previously on.
pub async fn update_segment_id(
    txn: &mut PgConnection,
    interface_id: MachineInterfaceId,
    segment_id: NetworkSegmentId,
    domain_id: Option<DomainId>,
) -> DatabaseResult<()> {
    let query = "UPDATE machine_interfaces SET segment_id = $1, domain_id = $2 WHERE id = $3";
    sqlx::query(query)
        .bind(segment_id)
        .bind(domain_id)
        .bind(interface_id)
        .execute(txn)
        .await
        .map(|_| ())
        .map_err(|e| DatabaseError::query(query, e))
}

/// Reconcile an existing interface's segment with the DHCP relay address.
///
/// - If the segments match, nothing happens.
/// - If the interface is on the static-assignments anchor segment with
///   no addresses (static was removed), move it to the relay's segment.
/// - If the interface is on static-assignments with addresses, leave it
///   alone -- the operator's static assignment takes priority over DHCP.
/// - If the interface is on a different managed segment, error -- this
///   is a real network mismatch (wrong VLAN/port).
async fn reconcile_interface_segment(
    txn: &mut PgConnection,
    existing_interface: &mut MachineInterfaceSnapshot,
    relays: &[IpAddr],
) -> DatabaseResult<()> {
    let relay_segments = crate::network_segment::for_relay_all(txn, relays).await?;

    if relay_segments.is_empty() {
        return Err(DatabaseError::internal(format!(
            "No network segment defined for DHCP relay addresses: {}",
            relays.iter().join(", ")
        )));
    };

    // If the existing interface belongs to any candidate segment, it is valid.
    // This handles proactive DPU ingestion where all admin gateways are candidates.
    if relay_segments
        .iter()
        .any(|n| n.id == existing_interface.segment_id)
    {
        return Ok(());
    }

    let on_static_assignments = existing_interface.segment_id
        == crate::network_segment::static_assignments(txn)
            .await
            .map(|s| s.id)
            .unwrap_or_default();

    // If the interface is on static-assignments with no addresses (as in
    // the static address was removed), move it to the relay's segment
    // so it can get a DHCP-allocated IP. The idea here being that someone
    // removed the static allocation on purpose, and now we're waiting for
    // the device to DHCP so we can see what segment it's coming in on.
    if on_static_assignments && existing_interface.addresses.is_empty() {
        let [relay_segment] = relay_segments.as_slice() else {
            return Err(DatabaseError::internal(format!(
                "Cannot move interface from static-assignments with multiple candidate relays: {} ",
                relays.iter().join(", ")
            )));
        };

        tracing::info!(
            mac_address = %existing_interface.mac_address,
            old_segment_id = %existing_interface.segment_id,
            new_segment_id = %relay_segment.id,
            "Moving interface from static-assignments into DHCP-managed segment"
        );
        update_segment_id(
            txn,
            existing_interface.id,
            relay_segment.id,
            relay_segment.subdomain_id,
        )
        .await?;
        existing_interface.segment_id = relay_segment.id;
    } else if on_static_assignments {
        // ...and if the interface is on static-assignments and still has
        // an addresse, the static assignment takes priority, so we leave
        // it as-is.
        tracing::debug!(
            mac_address = %existing_interface.mac_address,
            "Interface on static-assignments with addresses, leaving as-is"
        );
    } else {
        // And if it's a different managed segment, then yell. This logic
        // existing before the static-assigmnents and DHCP "reservation"
        // integration.
        return Err(DatabaseError::internal(format!(
            "Network segment mismatch for existing MAC address: {} expected: {} actual from network switch: {}",
            existing_interface.mac_address,
            existing_interface.segment_id,
            relay_segments
                .iter()
                .map(|ns| ns.id.to_string())
                .collect::<Vec<String>>()
                .join(", "),
        )));
    }

    Ok(())
}

/// Allocate new DHCP-based IP addresses for a specific address family
/// on an existing interface that has lost its addresses (e.g. after a
/// lease expiration, because maybe it was offline for a while, etc --
/// basically anything that caused a lease expiration to be cleaned up,
/// probably from ExpireDhcpLease being called). This uses the same
/// allocation logic that we use for allocating initial addresses, and
/// only allocates from prefixes matching the requested family (IPv4
/// or IPv6).
#[allow(txn_held_across_await)]
pub async fn allocate_address_for_family(
    txn: &mut PgConnection,
    interface_id: MachineInterfaceId,
    segment: &NetworkSegment,
    family: carbide_network::ip::IpAddressFamily,
) -> DatabaseResult<()> {
    let mut fast_txn = Transaction::begin_inner(txn).await?;
    lock_network_segment_shared(&mut fast_txn, segment).await?;

    for prefix in segment
        .prefixes
        .iter()
        .filter(|p| p.prefix.is_address_family(family))
    {
        let address = allocate_next_ip_with_retry(&mut fast_txn, segment, prefix).await?;
        insert_machine_interface_address(
            fast_txn.as_pgconn(),
            &interface_id,
            &address,
            AllocationType::Dhcp,
        )
        .await?;
    }

    fast_txn.commit().await?;
    Ok(())
}

/// Record that this interface just DHCPed, so it must still exist
pub async fn update_last_dhcp(
    txn: &mut PgConnection,
    interface_id: MachineInterfaceId,
    timestamp: Option<DateTime<Utc>>,
) -> Result<(), DatabaseError> {
    let query_timestamp = match timestamp {
        Some(t) => t,
        None => Utc::now(),
    };
    let query = "UPDATE machine_interfaces SET last_dhcp = $1::TIMESTAMPTZ WHERE id=$2::uuid";
    sqlx::query(query)
        .bind(query_timestamp.to_rfc3339())
        .bind(interface_id)
        .execute(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(())
}

pub async fn delete(
    interface_id: &MachineInterfaceId,
    txn: &mut PgConnection,
) -> Result<(), DatabaseError> {
    let query = "DELETE FROM machine_interfaces WHERE id=$1";
    crate::machine_interface_address::delete(txn, interface_id).await?;
    crate::dhcp_entry::delete(txn, interface_id).await?;
    sqlx::query(query)
        .bind(*interface_id)
        .execute(&mut *txn)
        .await
        .map(|_| ())
        .map_err(|e| DatabaseError::query(query, e))?;

    let query = "UPDATE machine_interfaces_deletion SET last_deletion=NOW() WHERE id = 1";
    sqlx::query(query)
        .bind(*interface_id)
        .execute(txn)
        .await
        .map(|_| ())
        .map_err(|e| DatabaseError::query(query, e))
}

pub async fn delete_by_ip(txn: &mut PgConnection, ip: IpAddr) -> Result<Option<()>, DatabaseError> {
    let interface = find_by_ip(&mut *txn, ip).await?;

    let Some(interface) = interface else {
        return Ok(None);
    };

    delete(&interface.id, txn).await?;

    Ok(Some(()))
}

/// Find all machine interface IDs associated with a switch.
pub async fn find_ids_by_switch_id(
    txn: &mut PgConnection,
    switch_id: &SwitchId,
) -> Result<Vec<MachineInterfaceId>, DatabaseError> {
    let query = "SELECT id FROM machine_interfaces WHERE switch_id = $1";
    sqlx::query_as::<_, MachineInterfaceId>(query)
        .bind(switch_id)
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

/// Find all machine interface IDs associated with a power shelf.
pub async fn find_ids_by_power_shelf_id(
    txn: &mut PgConnection,
    power_shelf_id: &PowerShelfId,
) -> Result<Vec<MachineInterfaceId>, DatabaseError> {
    let query = "SELECT id FROM machine_interfaces WHERE power_shelf_id = $1";
    sqlx::query_as::<_, MachineInterfaceId>(query)
        .bind(power_shelf_id)
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))
}

#[async_trait::async_trait]
impl<DB> UsedIpResolver<DB> for UsedAdminNetworkIpResolver
where
    for<'db> &'db mut DB: DbReader<'db>,
{
    // DEPRECATED
    // With the introduction of `used_prefixes()` this is no
    // longer an accurate approach for finding all allocated
    // IPs in a segment, since used_ips() completely ignores
    // the fact wider prefixes may have been allocated, even
    // though in the case of machine interfaces, its probably
    // always going to just be a /32.
    //
    // used_ips returns the used (or allocated) IPs for machine
    // interfaces in a given network segment.
    //
    // More specifically, this is intended to specifically
    // target the `address` column of the `machine_interface_addresses`
    // table, in which a single /32 is stored (although, as an
    // `inet`, it could techincally also have a prefix length).
    async fn used_ips(&self, txn: &mut DB) -> Result<Vec<IpAddr>, DatabaseError> {
        // IpAddrContainer is a small private struct used
        // for binding the result of the subsequent SQL
        // query, so we can implement FromRow and return
        // a Vec<IpAddr> a bit more easily.
        #[derive(FromRow)]
        struct IpAddrContainer {
            address: IpAddr,
        }

        let query = "
SELECT address FROM machine_interface_addresses
INNER JOIN machine_interfaces ON machine_interfaces.id = machine_interface_addresses.interface_id
INNER JOIN network_segments ON machine_interfaces.segment_id = network_segments.id
WHERE network_segments.id = $1::uuid";

        let containers: Vec<IpAddrContainer> = sqlx::query_as(query)
            .bind(self.segment_id)
            .fetch_all(txn)
            .await
            .map_err(|e| DatabaseError::query(query, e))?;

        let mut ips: Vec<IpAddr> = containers.iter().map(|c| c.address).collect();
        ips.extend(self.busy_ips.iter());
        Ok(ips)
    }

    // used_prefixes returns the used (or allocated) prefixes
    // for machine interfaces in a given network segment.
    //
    // NOTE(Chet): This is kind of a hack! Machine interfaces
    // aren't allocated prefixes other than a /32, and I think
    // it might be confusing if we added a `prefix` column to the
    // machine_interface_addresses table (since it's always
    // just going to be a /32 anyway).
    //
    // So, instead of database schema changes, this just gets all
    // of the used IPs and turns them into IpNetworks.
    //
    // This could also potentially just always return an error
    // saying its not implemented for machine_interfaces, BUT,
    // it keeps it cleaner knowing the IpAllocator works via
    // calling used_prefixes() regardless of who is using it.
    async fn used_prefixes(&self, txn: &mut DB) -> Result<Vec<IpNetwork>, DatabaseError> {
        let used_ips = self.used_ips(txn).await?;
        let mut ip_networks: Vec<IpNetwork> = Vec::new();
        for used_ip in used_ips {
            // Use /32 for IPv4 host addresses, /128 for IPv6 host addresses.
            let prefix_len = match used_ip {
                IpAddr::V4(_) => 32,
                IpAddr::V6(_) => 128,
            };
            let network = IpNetwork::new(used_ip, prefix_len).map_err(|e| {
                DatabaseError::new(
                    "machine_interface.used_prefixes",
                    sqlx::Error::Io(std::io::Error::other(e.to_string())),
                )
            })?;
            ip_networks.push(network);
        }
        Ok(ip_networks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_address_to_hostname_v4() {
        let address: IpAddr = "192.168.1.0".parse().unwrap();
        let hostname = address_to_hostname(&address).unwrap();
        assert_eq!("192-168-1-0", hostname);
    }

    #[test]
    fn test_address_to_hostname_v6() {
        let address: IpAddr = "2001:db8:abcd::2".parse().unwrap();
        let hostname = address_to_hostname(&address).unwrap();
        assert_eq!("2001-0db8-abcd-0000-0000-0000-0000-0002", hostname);
    }

    #[test]
    fn test_address_to_hostname_v6_loopback() {
        let address: IpAddr = "::1".parse().unwrap();
        let hostname = address_to_hostname(&address).unwrap();
        assert_eq!("0000-0000-0000-0000-0000-0000-0000-0001", hostname);
    }
}
