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
use std::collections::HashMap;
use std::net::IpAddr;
use std::ops::Deref;

use carbide_uuid::machine::MachineId;
use carbide_uuid::network::NetworkSegmentId;
use carbide_uuid::vpc::VpcId;
use config_version::ConfigVersion;
use futures::StreamExt;
use ipnetwork::IpNetwork;
use lazy_static::lazy_static;
use model::address_selection_strategy::AddressSelectionStrategy;
use model::controller_outcome::PersistentStateHandlerOutcome;
use model::network_segment::{
    NetworkDefinition, NetworkSegment, NetworkSegmentControllerState, NetworkSegmentSearchConfig,
    NetworkSegmentType, NewNetworkSegment,
};
use sqlx::{PgConnection, PgTransaction};

use crate::db_read::DbReader;
use crate::instance_address::UsedOverlayNetworkIpResolver;
use crate::ip_allocator::{IpAllocator, UsedIpResolver};
use crate::machine_interface::UsedAdminNetworkIpResolver;
use crate::{
    ColumnInfo, DatabaseError, DatabaseResult, FilterableQueryBuilder, ObjectColumnFilter,
};

#[derive(Copy, Clone)]
pub struct IdColumn;
impl ColumnInfo<'_> for IdColumn {
    type TableType = NetworkSegment;
    type ColumnType = NetworkSegmentId;

    fn column_name(&self) -> &'static str {
        "id"
    }
}

#[derive(Copy, Clone)]
pub struct VpcColumn;
impl ColumnInfo<'_> for VpcColumn {
    type TableType = NetworkSegment;
    type ColumnType = VpcId;

    fn column_name(&self) -> &'static str {
        "vpc_id"
    }
}

const NETWORK_SEGMENT_SNAPSHOT_QUERY_TEMPLATE: &str = r#"
     SELECT
        ns.*,
        COALESCE(prefixes_agg.json, '[]'::json) AS prefixes
        __HISTORY_SELECT__
     FROM network_segments ns
     LEFT JOIN LATERAL (
        SELECT np.segment_id,
            json_agg(np.*) AS json
        FROM network_prefixes np
        WHERE np.segment_id = ns.id
        GROUP BY np.segment_id
     ) AS prefixes_agg ON true
     __HISTORY_JOIN__
"#;

lazy_static! {
    static ref NETWORK_SEGMENT_SNAPSHOT_QUERY: String = NETWORK_SEGMENT_SNAPSHOT_QUERY_TEMPLATE
        .replace("__HISTORY_SELECT__", "")
        .replace("__HISTORY_JOIN__", "");

    static ref NETWORK_SEGMENT_SNAPSHOT_WITH_HISTORY_QUERY: String = NETWORK_SEGMENT_SNAPSHOT_QUERY_TEMPLATE
        .replace("__HISTORY_JOIN__", r#"
            LEFT JOIN LATERAL (
                SELECT h.segment_id,
                    json_agg(json_build_object('segment_id', h.segment_id, 'state', h.state::text, 'state_version', h.state_version, 'timestamp', h."timestamp")) AS json
                FROM network_segment_state_history h
                WHERE h.segment_id = ns.id
                GROUP BY h.segment_id
            ) AS history_agg ON true"#)
        .replace("__HISTORY_SELECT__", ", COALESCE(history_agg.json, '[]'::json) AS history");
}

pub async fn persist(
    value: NewNetworkSegment,
    txn: &mut PgConnection,
    initial_state: NetworkSegmentControllerState,
) -> Result<NetworkSegment, DatabaseError> {
    let version = ConfigVersion::initial();

    let query = "INSERT INTO network_segments (
                id,
                name,
                subdomain_id,
                vpc_id,
                mtu,
                version,
                controller_state_version,
                controller_state,
                vlan_id,
                vni_id,
                network_segment_type,
                can_stretch,
                allocation_strategy)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            RETURNING id";
    let segment_id: NetworkSegmentId = sqlx::query_as(query)
        .bind(value.id)
        .bind(&value.name)
        .bind(value.subdomain_id)
        .bind(value.vpc_id)
        .bind(value.mtu)
        .bind(version)
        .bind(version)
        .bind(sqlx::types::Json(&initial_state))
        .bind(value.vlan_id)
        .bind(value.vni)
        .bind(value.segment_type)
        .bind(value.can_stretch)
        .bind(value.allocation_strategy)
        .fetch_one(&mut *txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    crate::network_prefix::create_for(txn, &segment_id, &value.prefixes).await?;
    crate::state_history::persist(
        txn,
        crate::state_history::StateHistoryTableId::NetworkSegment,
        &segment_id,
        &initial_state,
        version,
    )
    .await?;

    find_by(
        txn,
        ObjectColumnFilter::One(IdColumn, &segment_id),
        Default::default(),
    )
    .await?
    .pop()
    .ok_or_else(|| {
        DatabaseError::new(
            "finding just-created network segment",
            sqlx::Error::RowNotFound,
        )
    })
}

pub async fn for_vpc(
    txn: impl DbReader<'_>,
    vpc_id: VpcId,
) -> Result<Vec<NetworkSegment>, DatabaseError> {
    lazy_static! {
        static ref query: String = format!(
            "{} WHERE ns.vpc_id=$1::uuid",
            NETWORK_SEGMENT_SNAPSHOT_QUERY.deref()
        );
    }
    let results: Vec<NetworkSegment> = {
        sqlx::query_as(&query)
            .bind(vpc_id)
            .fetch_all(txn)
            .await
            .map_err(|e| DatabaseError::query(&query, e))?
    };

    Ok(results)
}

pub async fn for_relay(
    txn: &mut PgConnection,
    relay: IpAddr,
) -> DatabaseResult<Option<NetworkSegment>> {
    lazy_static! {
        static ref query: String = format!(
            r#"{}
                INNER JOIN network_prefixes ON network_prefixes.segment_id = ns.id
                WHERE $1::inet <<= network_prefixes.prefix"#,
            NETWORK_SEGMENT_SNAPSHOT_QUERY.deref()
        );
    }
    let mut results = sqlx::query_as(&query)
        .bind(IpNetwork::from(relay))
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))?;

    match results.len() {
        0 | 1 => Ok(results.pop()),
        _ => Err(DatabaseError::internal(format!(
            "Multiple network segments defined for relay address {relay}"
        ))),
    }
}

pub async fn for_segment_type(
    txn: &mut PgConnection,
    relay: IpAddr,
    segment_type: NetworkSegmentType,
) -> DatabaseResult<Option<NetworkSegment>> {
    lazy_static! {
        static ref query: String = format!(
            r#"{}
                INNER JOIN network_prefixes ON network_prefixes.segment_id = ns.id
                WHERE $1::inet <<= network_prefixes.prefix
                AND $2 = ns.network_segment_type
                "#,
            NETWORK_SEGMENT_SNAPSHOT_QUERY.deref()
        );
    }
    let mut results = sqlx::query_as(&query)
        .bind(IpNetwork::from(relay))
        .bind(segment_type)
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::new(&query, e))?;

    if results.len() > 1 {
        tracing::trace!(
            "Multiple network segments defined for segment_type {} and relay address {}",
            segment_type.to_string(),
            relay.to_string()
        );
    }
    Ok(results.pop())
}

/// Retrieves the IDs of all network segments.
/// If `segment_type` is specified, only IDs of segments that match the specific type are returned.
pub async fn list_segment_ids(
    txn: &mut PgConnection,
    segment_type: Option<NetworkSegmentType>,
) -> Result<Vec<NetworkSegmentId>, DatabaseError> {
    let (query, mut segment_id_stream) = if let Some(segment_type) = segment_type {
        let query = "SELECT id FROM network_segments where network_segment_type=$1";
        let stream = sqlx::query_as(query).bind(segment_type).fetch(txn);
        (query, stream)
    } else {
        let query = "SELECT id FROM network_segments";
        let stream = sqlx::query_as(query).fetch(txn);
        (query, stream)
    };

    let mut results = Vec::new();
    while let Some(maybe_id) = segment_id_stream.next().await {
        let id = maybe_id.map_err(|e| DatabaseError::query(query, e))?;
        results.push(id);
    }

    Ok(results)
}
/// Fetch the stored definition for a single network, or `None` if never seeded.
pub async fn stored_def(
    txn: impl DbReader<'_>,
    name: &str,
) -> Result<Option<NetworkDefinition>, DatabaseError> {
    let query = "SELECT definition FROM network_def WHERE name = $1";
    let row: Option<(sqlx::types::Json<NetworkDefinition>,)> = sqlx::query_as(query)
        .bind(name)
        .fetch_optional(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(row.map(|(json,)| json.0))
}

/// Fetch every stored network definition as a `HashMap<name, def>`.
pub async fn all_stored_defs(
    txn: impl DbReader<'_>,
) -> Result<HashMap<String, NetworkDefinition>, DatabaseError> {
    let query = "SELECT name, definition FROM network_def";
    let rows: Vec<(String, sqlx::types::Json<NetworkDefinition>)> = sqlx::query_as(query)
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(rows
        .into_iter()
        .map(|(name, json)| (name, json.0))
        .collect())
}

/// Insert the `NetworkDefinition` snapshot for a network that has never been
/// seeded. Callers must check with `stored_def` / `all_stored_defs` before
/// calling this, and skip the insert when a snapshot is present.
pub async fn insert_network_def(
    txn: &mut PgConnection,
    name: &str,
    def: &NetworkDefinition,
) -> Result<(), DatabaseError> {
    let query = "INSERT INTO network_def (name, definition, seeded_at) VALUES ($1, $2, NOW())";
    let definition = serde_json::to_value(def).map_err(|e| {
        DatabaseError::InvalidArgument(format!(
            "NetworkDefinition: {def:?} could not be serialized to JSON: {e}"
        ))
    })?;

    sqlx::query(query)
        .bind(name)
        .bind(definition)
        .execute(&mut *txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(())
}

pub async fn segment_exists(txn: &mut PgConnection, name: &str) -> Result<bool, DatabaseError> {
    let query = "SELECT EXISTS(SELECT 1 FROM network_segments WHERE name = $1)";
    let (exists,): (bool,) = sqlx::query_as(query)
        .bind(name)
        .fetch_one(&mut *txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(exists)
}

/// Reconcile declared network definitions against what was previously seeded.
///
///   1. **New** (no snapshot, no segment): no-op. The caller's
///      segment-creation path is responsible for both creating the segment
///      and writing the snapshot.
///   2. **Backfill** (no snapshot, segment present): record the snapshot.
///   3. **In sync** (snapshot matches declaration): no-op.
///   4. **Drift** (snapshot differs from declaration, segment present):
///      warn and leave both in place. Operator must reconcile by hand.
///   5. **Anomaly** (snapshot present, segment absent): error-log and skip.
///      Indicates a partial restore or manual deletion.
///
/// Networks that appear in the snapshot table but are no longer declared
/// ("dropped" from `InitialObjectsConfig.networks`) are warned about, but
/// not removed.
pub async fn reconcile_network_defs(
    txn: &mut PgConnection,
    declared: &HashMap<String, NetworkDefinition>,
) -> Result<(), DatabaseError> {
    let stored = all_stored_defs(&mut *txn).await?;

    for (name, def) in declared {
        let exists = segment_exists(&mut *txn, name).await?;
        match (stored.get(name), exists) {
            // Snapshot exists but network segment is missing from network_segments.
            // Anomaly: e.g. partial DB restore.  Don't auto-heal - operator must reconcile by hand
            (Some(stored_def), false) => {
                tracing::error!(
                    network_name = name,
                    stored = ?stored_def,
                    "NetworkDefinition snapshot exists but network_segments has no rows for it; \
                     manual recovery required"
                );
            }
            // Already seeded with the current declaration
            (Some(stored_def), true) if stored_def == def => {}
            // Declaration has drifted since seed. Warn don't reapply
            (Some(stored_def), true) => {
                tracing::warn!(
                    network_name = name,
                    stored = ?stored_def,
                    declared = ?def,
                    "NetworkDefinition has changed since it was seeded; not re-applying"
                );
            }
            // Network segment exists, but has no snapshot yet.
            // Pre-migration deployment or a network was re-added after a snapshot was
            // manually deleted.  Record the snapshot only
            (None, true) => {
                insert_network_def(txn, name, def).await?;
                tracing::info!(
                    network_name = name,
                    "Backfilled NetworkDefinition snapshot for pre-existing network segment"
                );
            }
            // New networks are seeded by the caller (`create_initial_networks`),
            // which both expands the definition into a `NewNetworkSegment` and
            // writes the snapshot in the same transaction.
            (None, false) => {}
        }
    }

    for name in stored.keys() {
        if !declared.contains_key(name) {
            tracing::warn!(
                network_name = name,
                "Network segment exists in database but is no longer declared in any config file"
            );
        }
    }

    Ok(())
}
pub async fn find_ids(
    txn: impl DbReader<'_>,
    filter: model::network_segment::NetworkSegmentSearchFilter,
) -> Result<Vec<NetworkSegmentId>, DatabaseError> {
    // build query
    let mut builder = sqlx::QueryBuilder::new("SELECT s.id FROM network_segments AS s");
    let mut has_filter = false;
    if let Some(tenant_org_id) = &filter.tenant_org_id {
        builder.push(" JOIN vpcs AS v ON s.vpc_id = v.id WHERE v.organization_id = ");
        builder.push_bind(tenant_org_id);
        has_filter = true;
    }
    if let Some(name) = &filter.name {
        if has_filter {
            builder.push(" AND s.name = ");
        } else {
            builder.push(" WHERE s.name = ");
        }
        builder.push_bind(name);
    }

    let query = builder.build_query_as();
    let ids: Vec<NetworkSegmentId> = query
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::new("network_segment::find_ids", e))?;

    Ok(ids)
}

pub async fn find_by<'a, C: ColumnInfo<'a, TableType = NetworkSegment>, DB>(
    conn: &mut DB,
    filter: ObjectColumnFilter<'a, C>,
    search_config: NetworkSegmentSearchConfig,
) -> Result<Vec<NetworkSegment>, DatabaseError>
where
    for<'db> &'db mut DB: DbReader<'db>,
{
    let mut query = FilterableQueryBuilder::new(if search_config.include_history {
        NETWORK_SEGMENT_SNAPSHOT_WITH_HISTORY_QUERY.deref()
    } else {
        NETWORK_SEGMENT_SNAPSHOT_QUERY.deref()
    })
    .filter(&filter);

    let mut all_records = query
        .build_query_as()
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| DatabaseError::query(query.sql(), e))?;

    if search_config.include_num_free_ips {
        update_num_free_ips_into_prefix_list(conn, &mut all_records).await?;
    }
    Ok(all_records)
}

/// Find network segments attached to a machine through machine_interfaces, optionally of a certain type
pub async fn find_ids_by_machine_id(
    txn: &mut PgConnection,
    machine_id: &::carbide_uuid::machine::MachineId,
    network_segment_type: Option<NetworkSegmentType>,
) -> Result<Vec<NetworkSegmentId>, DatabaseError> {
    let result = batch_find_ids_by_machine_ids(txn, &[*machine_id], network_segment_type).await?;

    Ok(result.get(machine_id).cloned().unwrap_or_default())
}

/// Batch find network segments attached to multiple machines through machine_interfaces.
/// Returns a HashMap mapping each machine ID to its list of segment IDs.
pub async fn batch_find_ids_by_machine_ids(
    txn: &mut PgConnection,
    machine_ids: &[MachineId],
    network_segment_type: Option<NetworkSegmentType>,
) -> Result<HashMap<MachineId, Vec<NetworkSegmentId>>, DatabaseError> {
    if machine_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut query = sqlx::QueryBuilder::new(
        r#"SELECT mi.machine_id, ns.id FROM machines m
                LEFT JOIN machine_interfaces mi ON (mi.machine_id = m.id)
                INNER JOIN network_segments ns ON (ns.id = mi.segment_id)
                WHERE mi.machine_id = ANY("#,
    );

    query.push_bind(
        machine_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
    );
    query.push(")");

    if let Some(network_segment_type) = network_segment_type {
        query
            .push(" AND ns.network_segment_type = ")
            .push_bind(network_segment_type);
    }

    let rows: Vec<(String, NetworkSegmentId)> = query
        .build_query_as()
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(query.sql(), e))?;

    let mut result: HashMap<MachineId, Vec<NetworkSegmentId>> = HashMap::new();
    for (machine_id_str, segment_id) in rows {
        if let Ok(machine_id) = machine_id_str.parse::<MachineId>() {
            result.entry(machine_id).or_default().push(segment_id);
        }
    }

    Ok(result)
}

async fn update_num_free_ips_into_prefix_list<DB>(
    conn: &mut DB,
    all_records: &mut [NetworkSegment],
) -> Result<(), DatabaseError>
where
    for<'db> &'db mut DB: DbReader<'db>,
{
    for record in all_records.iter_mut().filter(|s| !s.prefixes.is_empty()) {
        let mut busy_ips = vec![];
        for prefix in &record.prefixes {
            if let Some(svi_ip) = prefix.svi_ip {
                busy_ips.push(svi_ip);
            }
        }
        let dhcp_handler: Box<dyn UsedIpResolver<DB> + Send> = if record.segment_type.is_tenant() {
            // Note on UsedOverlayNetworkIpResolver:
            // In this case, the IpAllocator isn't being used to iterate to get
            // the next available prefix_length allocation -- it's actually just
            // being used to get the number of free IPs left in a given tenant
            // network segment, so just hard-code a /32 prefix_length. NOW.. on
            // one hand, you could say the prefix_length doesn't matter here,
            // because this is really just here to get the number of free IPs left
            // in a network segment. BUT, on the other hand, do we care about the
            // number of free IPs left, or the number of free instance allocations
            // left? For example, if we're allocating /30's, we might be more
            // interested in knowing we can allocate 4 more machines (and not 16
            // more IPs).
            Box::new(UsedOverlayNetworkIpResolver {
                segment_id: record.id,
                busy_ips,
            })
        } else {
            // Note on UsedAdminNetworkIpResolver:
            // In this case, the IpAllocator isn't being used to iterate to get
            // the next available prefix_length allocation -- it's actually just
            // being used to get the number of free IPs left in a given admin
            // network segment, so just hard-code a /32 prefix_length. Unlike the
            // tenant segments, the admin segments are always (at least for the
            // foreseeable future) just going to allocate a /32 for the machine
            // interface.
            Box::new(UsedAdminNetworkIpResolver {
                segment_id: record.id,
                busy_ips,
            })
        };

        let mut allocated_addresses = IpAllocator::new(
            &mut *conn,
            record,
            dhcp_handler,
            AddressSelectionStrategy::NextAvailableIp,
        )
        .await
        .map_err(|e| {
            DatabaseError::new(
                "IpAllocator.new error",
                sqlx::Error::Io(std::io::Error::other(e.to_string())),
            )
        })?;

        let nfree = allocated_addresses.num_free().map_err(|e| {
            DatabaseError::new(
                "IpAllocator.num_free error",
                sqlx::Error::Io(std::io::Error::other(e.to_string())),
            )
        })?;

        record.prefixes[0].num_free_ips = nfree;
    }

    Ok(())
}

/// Updates the network segment state that is owned by the state controller
/// under the premise that the current controller state version didn't change.
///
/// Returns `true` if the state could be updated, and `false` if the object
/// either doesn't exist anymore or is at a different version.
pub async fn try_update_controller_state(
    txn: &mut PgConnection,
    segment_id: NetworkSegmentId,
    expected_version: ConfigVersion,
    new_version: ConfigVersion,
    new_state: &NetworkSegmentControllerState,
) -> Result<bool, DatabaseError> {
    let query = "UPDATE network_segments SET controller_state_version=$1, controller_state=$2::json where id=$3::uuid AND controller_state_version=$4 returning id";
    let result = sqlx::query_as::<_, NetworkSegmentId>(query)
        .bind(new_version)
        .bind(sqlx::types::Json(new_state))
        .bind(segment_id)
        .bind(expected_version)
        .fetch_optional(&mut *txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;

    Ok(result.is_some())
}

pub async fn update_controller_state_outcome(
    txn: &mut PgConnection,
    segment_id: NetworkSegmentId,
    outcome: PersistentStateHandlerOutcome,
) -> Result<(), DatabaseError> {
    let query = "UPDATE network_segments SET controller_state_outcome=$1::json WHERE id=$2";
    sqlx::query(query)
        .bind(sqlx::types::Json(outcome))
        .bind(segment_id)
        .execute(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(())
}

pub async fn set_vpc_id_and_can_stretch(
    value: &NetworkSegment,
    txn: &mut PgConnection,
    vpc_id: VpcId,
) -> Result<(), DatabaseError> {
    let query = "UPDATE network_segments SET vpc_id=$1, can_stretch=true WHERE id=$2";
    sqlx::query(query)
        .bind(vpc_id)
        .bind(value.id)
        .execute(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;
    Ok(())
}

pub async fn mark_as_deleted(
    value: &NetworkSegment,
    txn: &mut PgConnection,
) -> DatabaseResult<NetworkSegmentId> {
    // This check is not strictly necessary here, since the segment state machine
    // will also wait until all allocated addresses have been freed before actually
    // deleting the segment. However it gives the user some early feedback for
    // the commmon case, which allows them to free resources
    let num_machine_interfaces =
        crate::machine_interface::count_by_segment_id(txn, &value.id).await?;
    if num_machine_interfaces > 0 {
        return DatabaseResult::Err(DatabaseError::NetworkSegmentDelete(
            "Network Segment can't be deleted with associated MachineInterface".to_string(),
        ));
    }
    let num_instance_addresses =
        crate::instance_address::count_by_segment_id(txn, &value.id).await?;
    if num_instance_addresses > 0 {
        return DatabaseResult::Err(DatabaseError::NetworkSegmentDelete(
            "Network Segment can't be deleted while addresses on the segment are allocated to instances".to_string(),
        ));
    }

    let query = "UPDATE network_segments SET updated=NOW(), deleted=NOW() WHERE id=$1 RETURNING id";
    let id = sqlx::query_as(query)
        .bind(value.id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;

    Ok(id)
}

pub async fn final_delete(
    segment_id: NetworkSegmentId,
    txn: &mut PgConnection,
) -> Result<NetworkSegmentId, DatabaseError> {
    crate::network_prefix::delete_for_segment(segment_id, txn).await?;

    let query = "DELETE FROM network_segments WHERE id=$1::uuid RETURNING id";
    let segment: NetworkSegmentId = sqlx::query_as(query)
        .bind(segment_id)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;

    Ok(segment)
}

pub async fn find_by_name(
    txn: &mut PgConnection,
    name: &str,
) -> Result<NetworkSegment, DatabaseError> {
    lazy_static! {
        static ref query: String =
            format!("{} WHERE name = $1", NETWORK_SEGMENT_SNAPSHOT_QUERY.deref());
    }
    sqlx::query_as(&query)
        .bind(name)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))
}

/// Well-known name for the static assignments "anchor segment",
/// making it extra-obvious that it's a special one.
pub const STATIC_ASSIGNMENTS_SEGMENT_NAME: &str = "static-assignments";

/// Returns the static-assignments anchor segment, used for external
/// static IP assignments that don't fall within any managed network prefix.
pub async fn static_assignments(txn: &mut PgConnection) -> Result<NetworkSegment, DatabaseError> {
    find_by_name(txn, STATIC_ASSIGNMENTS_SEGMENT_NAME).await
}

/// This method returns Admin network segment.
pub async fn admin(txn: &mut PgConnection) -> Result<NetworkSegment, DatabaseError> {
    lazy_static! {
        static ref query: String = format!(
            "{} WHERE network_segment_type = 'admin'",
            NETWORK_SEGMENT_SNAPSHOT_QUERY.deref()
        );
    }
    let mut segments: Vec<NetworkSegment> = sqlx::query_as(&query)
        .fetch_all(txn)
        .await
        .map_err(|e| DatabaseError::query(&query, e))?;

    if segments.is_empty() {
        return Err(DatabaseError::query(&query, sqlx::Error::RowNotFound));
    }

    Ok(segments.remove(0))
}

/// Are queried segment in ready state?
/// Returns true if all segments are in Ready state, else false
pub async fn are_network_segments_ready<DB>(
    conn: &mut DB,
    segment_ids: &[NetworkSegmentId],
) -> Result<bool, DatabaseError>
where
    for<'db> &'db mut DB: DbReader<'db>,
{
    let segments = find_by(
        conn,
        ObjectColumnFilter::List(IdColumn, segment_ids),
        NetworkSegmentSearchConfig::default(),
    )
    .await?;

    Ok(!segments
        .iter()
        .any(|x| x.controller_state.value != NetworkSegmentControllerState::Ready))
}

/// This function is different from `mark_as_deleted` as no validation is checked here and it
/// takes a list of ids to reduce db handling time.
/// Instance is already deleted immediately before this.
pub async fn mark_as_deleted_no_validation(
    txn: &mut PgConnection,
    network_segment_ids: &[NetworkSegmentId],
) -> DatabaseResult<NetworkSegmentId> {
    let query =
        "UPDATE network_segments SET updated=NOW(), deleted=NOW() WHERE id=ANY($1) RETURNING id";
    let id = sqlx::query_as(query)
        .bind(network_segment_ids)
        .fetch_one(txn)
        .await
        .map_err(|e| DatabaseError::query(query, e))?;

    Ok(id)
}

/// SVI IP is needed for Network Segments attached to FNN VPCs.
/// Usually third IP of a prefix is used as SVI IP. In case, first 3 IPs are not reserved,
/// carbide will pick any available free IP and store it in DB for further use.
/// Allocates SVI IPs for all prefixes in a segment that don't already have one.
/// For dual-stack segments, this allocates one SVI IP per prefix (v4 and v6).
///
/// For each prefix, the SVI IP is the 3rd address in the prefix (e.g. 10.0.1.2
/// in 10.0.1.0/24, or 2001:db8::2 in 2001:db8::/112). This address is shared
/// across all DPUs via VRR.
pub async fn allocate_svi_ip(
    value: &NetworkSegment,
    txn: &mut PgTransaction<'_>,
) -> Result<IpAddr, DatabaseError> {
    let mut first_svi_ip = None;

    for prefix in &value.prefixes {
        if prefix.svi_ip.is_some() {
            if first_svi_ip.is_none() {
                first_svi_ip = prefix.svi_ip;
            }
            continue;
        }

        // For prefixes with num_reserved >= 3, the 3rd IP is guaranteed reserved
        // and safe to use as the SVI IP. For smaller prefixes, the 3rd IP may
        // already be allocated to an instance, so we fall back to the IP allocator
        // to find the next free address in this specific prefix.
        let svi_ip = if prefix.num_reserved >= 3 {
            prefix.prefix.iter().nth(2).ok_or_else(|| {
                DatabaseError::internal(format!("Prefix {} does not have 3 valid IPs.", prefix.id))
            })?
        } else {
            // Build a single-prefix segment view so the allocator only looks
            // at this prefix (avoids the cross-prefix bug for dual-stack).
            let single_prefix_segment = NetworkSegment {
                prefixes: vec![prefix.clone()],
                ..value.clone()
            };
            let (_, svi_ip) = if !value.segment_type.is_tenant() {
                crate::machine_interface::allocate_svi_ip(txn, &single_prefix_segment).await?
            } else {
                crate::instance_address::allocate_svi_ip(txn, &single_prefix_segment).await?
            };
            svi_ip
        };

        crate::network_prefix::set_svi_ip(txn, prefix.id, &svi_ip).await?;

        if first_svi_ip.is_none() {
            first_svi_ip = Some(svi_ip);
        }
    }

    first_svi_ip.ok_or_else(|| DatabaseError::NotFoundError {
        kind: "prefix",
        id: value.id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use model::network_segment::NetworkDefinitionSegmentType;

    use super::*;

    // Insert just enough into `network_segments` to make
    // `segment_exists(name)` all other columns have DEFAULTs;
    // only `version` is VARCHAR(64) NOT NULL with no default, so we
    // supply a placeholder.
    async fn minimum_segment_data(pool: &sqlx::PgPool, name: &str) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO network_segments (name, version) VALUES ($1, 'V1-T0')")
            .bind(name)
            .execute(pool)
            .await?;
        Ok(())
    }
    // A brand-new network is declared but no segment exists yet and no
    // snapshot has been recorded.
    // (`create_initial_networks`) is responsible for inserting both the
    // segment and the snapshot in the same transaction.
    #[crate::sqlx_test]
    async fn test_reconcile_network_defs_brand_new_is_noop(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let def = NetworkDefinition {
            segment_type: NetworkDefinitionSegmentType::Admin,
            prefix: "192.168.1.0/24".to_string(),
            gateway: "192.168.1.1".to_string(),
            mtu: 1500,
            reserve_first: 5,
            allocation_strategy: Default::default(),
        };

        let mut txn = pool.begin().await?;
        let declared: HashMap<String, NetworkDefinition> = [("brand-new".to_string(), def.clone())]
            .into_iter()
            .collect();

        reconcile_network_defs(&mut txn, &declared).await?;

        // Reconcile must not have written a snapshot for the brand-new entry.
        let stored = stored_def(txn.as_mut(), "brand-new").await?;
        assert!(
            stored.is_none(),
            "reconcile must leave brand-new networks alone; \
             snapshot insertion is the caller's responsibility"
        );

        // And must not have created a network_segments row either.
        assert!(
            !segment_exists(&mut txn, "brand-new").await?,
            "reconcile must not create a network_segments row for a brand-new network"
        );

        txn.rollback().await?;
        Ok(())
    }

    // Test-only constructor for a `NetworkDefinition` with sensible defaults
    fn def(prefix: &str, gateway: &str) -> NetworkDefinition {
        NetworkDefinition {
            segment_type: NetworkDefinitionSegmentType::Admin,
            prefix: prefix.to_string(),
            gateway: gateway.to_string(),
            mtu: 1500,
            reserve_first: 3,
            allocation_strategy: Default::default(),
        }
    }

    fn declared_one(name: &str, def: NetworkDefinition) -> HashMap<String, NetworkDefinition> {
        [(name.to_string(), def)].into_iter().collect()
    }

    // A segment row exists in `network_segments` but has no `network_def`
    // snapshot
    // Reconcile must record the snapshot without re-creating the segment.
    #[crate::sqlx_test]
    async fn reconcile_network_defs_backfills_existing_segment(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        minimum_segment_data(&pool, "pre-existing").await?;

        let mut txn = pool.begin().await?;
        let def = def("192.168.1.0/24", "192.168.1.1");

        reconcile_network_defs(&mut txn, &declared_one("pre-existing", def.clone())).await?;

        let stored = stored_def(txn.as_mut(), "pre-existing").await?;
        assert_eq!(stored.as_ref(), Some(&def), "snapshot must be backfilled");
        txn.rollback().await?;
        Ok(())
    }

    // Segment + snapshot both already exist and match the declaration
    #[crate::sqlx_test]
    async fn reconcile_network_defs_in_sync_is_noop(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        minimum_segment_data(&pool, "stable").await?;

        let mut txn = pool.begin().await?;
        let def = def("192.168.1.0/24", "192.168.1.1");
        insert_network_def(&mut txn, "stable", &def).await?;

        reconcile_network_defs(&mut txn, &declared_one("stable", def.clone())).await?;

        let stored = stored_def(txn.as_mut(), "stable").await?;
        assert_eq!(
            stored.as_ref(),
            Some(&def),
            "in-sync snapshot must be left untouched",
        );
        txn.rollback().await?;
        Ok(())
    }

    // Segment + snapshot exist, but the declared definition has drifted
    // since seed. Reconcile must warn and leave the stored snapshot alone,
    // not silently reapply the new declaration.
    #[crate::sqlx_test]
    async fn reconcile_network_defs_drift_does_not_apply(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        minimum_segment_data(&pool, "drifty").await?;

        let mut txn = pool.begin().await?;
        let original = def("192.168.1.0/24", "192.168.168.1");
        let drifted = def("10.0.0.0/24", "10.0.0.1");
        insert_network_def(&mut txn, "drifty", &original).await?;

        reconcile_network_defs(&mut txn, &declared_one("drifty", drifted.clone())).await?;

        let stored = stored_def(txn.as_mut(), "drifty").await?;
        assert_eq!(
            stored.as_ref(),
            Some(&original),
            "drift path must not overwrite the stored snapshot",
        );
        txn.rollback().await?;
        Ok(())
    }

    // Snapshot exists, but no segment row — indicates a partial restore or
    // manual deletion. Reconcile must error.
    #[crate::sqlx_test]
    async fn reconcile_network_defs_snapshot_without_segment_logs_error(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut txn = pool.begin().await?;
        let def = def("192.168.1.0/24", "192.168.1.1");
        insert_network_def(&mut txn, "orphan-snapshot", &def).await?;

        reconcile_network_defs(&mut txn, &declared_one("orphan-snapshot", def.clone())).await?;

        // Snapshot is preserved; no segment was created in recovery.
        let stored = stored_def(txn.as_mut(), "orphan-snapshot").await?;
        assert_eq!(
            stored.as_ref(),
            Some(&def),
            "anomaly path must leave the snapshot alone",
        );
        assert!(
            !segment_exists(&mut txn, "orphan-snapshot").await?,
            "anomaly path must not auto-create a network_segments row",
        );
        txn.rollback().await?;
        Ok(())
    }

    // A snapshot exists for a network that is no longer mentioned in any
    // declared config — typical of an operator removing the definition.
    // Reconcile must warn but not delete the snapshot
    #[crate::sqlx_test]
    async fn reconcile_network_defs_dropped_declaration_is_orphaned(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        minimum_segment_data(&pool, "abandoned").await?;

        let mut txn = pool.begin().await?;
        let def = def("192.168.1.0/24", "192.168.1.1");
        insert_network_def(&mut txn, "abandoned", &def).await?;

        let empty: HashMap<String, NetworkDefinition> = HashMap::new();
        reconcile_network_defs(&mut txn, &empty).await?;

        let stored = stored_def(txn.as_mut(), "abandoned").await?;
        assert_eq!(
            stored.as_ref(),
            Some(&def),
            "dropped declarations must not be deleted from network_def",
        );
        assert!(
            segment_exists(&mut txn, "abandoned").await?,
            "dropped declarations must not be deleted from network_segments",
        );
        txn.rollback().await?;
        Ok(())
    }
}
