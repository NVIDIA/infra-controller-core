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

use std::str::FromStr;
use std::sync::Arc;

use askama::Template;
use axum::extract::{self, Path as AxumPath, State as AxumState};
use axum::response::{Html, IntoResponse, Response};
use carbide_uuid::machine::MachineId;
use carbide_uuid::rack::RackId;
use health_report::HealthReport;
use hyper::http::StatusCode;
use rpc::forge::forge_server::Forge;
use rpc::forge::{
    HealthReportApplyMode, InsertMachineHealthReportRequest, InsertRackHealthReportRequest,
    ListRackHealthReportsRequest, MachinesByIdsRequest, RacksByIdsRequest,
    RemoveMachineHealthReportRequest, RemoveRackHealthReportRequest,
};

use super::{Base, filters};
use crate::api::Api;
use crate::auth::AuthContext;

#[derive(Template)]
#[template(path = "health_reports_detail.html")]
struct HealthPage {
    id: String,
    object_label: String,
    object_detail_url: String,
    show_machine_templates: bool,
    entries: Vec<HealthReportEntry>,
    aggregate_health: LabeledHealthReport,
    health_contributors: Vec<LabeledHealthReport>,
    history_url: Option<String>,
    history: HealthHistoryTable,
}

#[derive(Template)]
#[template(path = "health_history_table.html")]
pub(super) struct HealthHistoryTable {
    pub records: Vec<HealthHistoryRecord>,
}

#[derive(Debug, serde::Serialize)]
pub(super) struct HealthHistoryRecord {
    pub timestamp: String,
    pub health: health_report::HealthReport,
}

impl HealthHistoryRecord {
    pub fn from_rpc_convert_invalid(record: ::rpc::forge::HealthHistoryRecord) -> Self {
        HealthHistoryRecord {
            timestamp: record.time.map(|time| time.to_string()).unwrap_or_default(),
            health: record
                .health
                .map(health_report_from_rpc_convert_invalid)
                .unwrap_or_else(health_report::HealthReport::missing_report),
        }
    }
}

#[derive(Template)]
#[template(path = "machine_health_component.html")]
struct LabeledHealthReport {
    label: String,
    report: Option<health_report::HealthReport>,
}

#[derive(Clone)]
enum HealthObject {
    Machine(MachineId),
    Rack(RackId),
}

impl HealthObject {
    fn id(&self) -> String {
        match self {
            HealthObject::Machine(id) => id.to_string(),
            HealthObject::Rack(id) => id.to_string(),
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            HealthObject::Machine(_) => "machine",
            HealthObject::Rack(_) => "rack",
        }
    }

    fn label(&self) -> String {
        match self {
            HealthObject::Machine(id) => id.machine_type().to_string(),
            HealthObject::Rack(_) => "Rack".to_string(),
        }
    }

    fn detail_url(&self) -> String {
        format!("/admin/{}/{}", self.kind(), self.id())
    }

    fn history_url(&self) -> Option<String> {
        match self {
            HealthObject::Machine(id) => Some(format!("/admin/machine/{id}/health-history")),
            HealthObject::Rack(_) => None,
        }
    }

    fn show_machine_templates(&self) -> bool {
        matches!(self, HealthObject::Machine(_))
    }
}

struct HealthPageData {
    entries: Vec<::rpc::forge::HealthReportEntry>,
    aggregate_health: Option<HealthReport>,
    health_contributors: Vec<LabeledHealthReport>,
    history: HealthHistoryTable,
}

struct MachineHealthSnapshot {
    aggregate_health: Option<HealthReport>,
    associated_dpu_machine_ids: Vec<MachineId>,
}

/// View machine health.
pub async fn machine_health(
    AxumState(state): AxumState<Arc<Api>>,
    AxumPath(machine_id): AxumPath<String>,
) -> Response {
    let Ok(machine_id) = MachineId::from_str(&machine_id) else {
        return (StatusCode::BAD_REQUEST, "invalid machine id").into_response();
    };
    if machine_id.machine_type().is_dpu() {
        return (
            StatusCode::NOT_FOUND,
            "no health for dpu. see host machine instead",
        )
            .into_response();
    }

    let data = match fetch_machine_health_page_data(&state, &machine_id).await {
        Ok(data) => data,
        Err(response) => return response,
    };

    render_health(HealthObject::Machine(machine_id), data)
}

/// View rack health.
pub async fn rack_health(
    AxumState(state): AxumState<Arc<Api>>,
    AxumPath(rack_id): AxumPath<String>,
) -> Response {
    let Ok(rack_id) = RackId::from_str(&rack_id) else {
        return (StatusCode::BAD_REQUEST, "invalid rack id").into_response();
    };

    let data = match fetch_rack_health_page_data(&state, &rack_id).await {
        Ok(data) => data,
        Err(response) => return response,
    };

    render_health(HealthObject::Rack(rack_id), data)
}

fn render_health(object: HealthObject, data: HealthPageData) -> Response {
    let mut entries = data.entries;
    // Sort by type first and source name second.
    entries.sort_by(|a, b| {
        if a.mode() == HealthReportApplyMode::Replace {
            return std::cmp::Ordering::Less;
        } else if b.mode() == HealthReportApplyMode::Replace {
            return std::cmp::Ordering::Greater;
        }
        a.report
            .as_ref()
            .map(|a| &a.source)
            .cmp(&b.report.as_ref().map(|b| &b.source))
    });

    let entries: Vec<HealthReportEntry> = entries
        .into_iter()
        .map(HealthReportEntry::from_rpc_convert_invalid)
        .collect();

    let display = HealthPage {
        id: object.id(),
        object_label: object.label(),
        object_detail_url: object.detail_url(),
        show_machine_templates: object.show_machine_templates(),
        aggregate_health: LabeledHealthReport {
            label: "Aggregate Health".to_string(),
            report: data.aggregate_health,
        },
        health_contributors: data.health_contributors,
        entries,
        history_url: object.history_url(),
        history: data.history,
    };

    (StatusCode::OK, Html(display.render().unwrap())).into_response()
}

async fn fetch_machine_health_page_data(
    api: &Api,
    machine_id: &MachineId,
) -> Result<HealthPageData, Response> {
    let snapshot = fetch_machine_health_snapshot(api, machine_id).await?;
    let entries = match list_machine_health_report_entries(api, machine_id).await {
        Ok(entries) => entries,
        Err(err) if err.code() == tonic::Code::NotFound => Vec::new(),
        Err(err) => {
            tracing::error!(%err, %machine_id, "list_health_report_entries");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Html(err.to_string())).into_response());
        }
    };
    let health_contributors =
        fetch_dpu_health_contributors(api, machine_id, snapshot.associated_dpu_machine_ids).await?;
    let history = match fetch_health_history(api, machine_id).await {
        Ok(records) => HealthHistoryTable { records },
        Err(err) => {
            tracing::error!(%err, %machine_id, "find_machine_health_histories");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Html(err.to_string())).into_response());
        }
    };

    Ok(HealthPageData {
        entries,
        aggregate_health: snapshot.aggregate_health,
        health_contributors,
        history,
    })
}

async fn fetch_rack_health_page_data(
    api: &Api,
    rack_id: &RackId,
) -> Result<HealthPageData, Response> {
    let aggregate_health = fetch_rack_aggregate_health(api, rack_id).await?;
    let entries = match list_rack_health_report_entries(api, rack_id).await {
        Ok(entries) => entries,
        Err(err) if err.code() == tonic::Code::NotFound => Vec::new(),
        Err(err) => {
            tracing::error!(%err, %rack_id, "list_rack_health_report_overrides");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Html(err.to_string())).into_response());
        }
    };

    Ok(HealthPageData {
        entries,
        aggregate_health,
        health_contributors: Vec::new(),
        history: HealthHistoryTable {
            records: Vec::new(),
        },
    })
}

async fn fetch_dpu_health_contributors(
    api: &Api,
    host_machine_id: &MachineId,
    dpu_machine_ids: Vec<MachineId>,
) -> Result<Vec<LabeledHealthReport>, Response> {
    if dpu_machine_ids.is_empty() {
        return Ok(Vec::new());
    }

    let request = tonic::Request::new(MachinesByIdsRequest {
        machine_ids: dpu_machine_ids,
        include_history: false,
    });
    let dpus = match api
        .find_machines_by_ids(request)
        .await
        .map(|response| response.into_inner())
    {
        Ok(m) => m.machines,
        Err(err) if err.code() == tonic::Code::NotFound => Vec::new(),
        Err(err) => {
            tracing::error!(%err, %host_machine_id, "find_dpu_machines_by_ids");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Html(err.to_string())).into_response());
        }
    };

    Ok(dpus
        .into_iter()
        .map(|dpu| LabeledHealthReport {
            label: format!(
                "DPU Health {}",
                dpu.id.map(|id| id.to_string()).unwrap_or_default()
            ),
            report: dpu.health.map(health_report_from_rpc_convert_invalid),
        })
        .collect())
}

async fn list_machine_health_report_entries(
    api: &Api,
    machine_id: &MachineId,
) -> Result<Vec<::rpc::forge::HealthReportEntry>, tonic::Status> {
    Ok(api
        .list_machine_health_reports(tonic::Request::new(*machine_id))
        .await?
        .into_inner()
        .health_report_entries)
}

async fn list_rack_health_report_entries(
    api: &Api,
    rack_id: &RackId,
) -> Result<Vec<::rpc::forge::HealthReportEntry>, tonic::Status> {
    Ok(api
        .list_rack_health_reports(tonic::Request::new(ListRackHealthReportsRequest {
            rack_id: Some(rack_id.clone()),
        }))
        .await?
        .into_inner()
        .health_report_entries)
}

async fn fetch_machine_health_snapshot(
    api: &Api,
    machine_id: &MachineId,
) -> Result<MachineHealthSnapshot, Response> {
    let machine = match api
        .find_machines_by_ids(tonic::Request::new(rpc::forge::MachinesByIdsRequest {
            machine_ids: vec![*machine_id],
            include_history: false,
        }))
        .await
        .map(|response| response.into_inner())
    {
        Ok(m) if m.machines.is_empty() => None,
        Ok(m) if m.machines.len() != 1 => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!(
                    "Machine list for {machine_id} returned {} machines",
                    m.machines.len()
                ),
            )
                .into_response());
        }
        Ok(mut m) => Some(m.machines.remove(0)),
        Err(err) if err.code() == tonic::Code::NotFound => None,
        Err(err) => {
            tracing::error!(%err, %machine_id, "find_machines_by_ids");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Html(err.to_string())).into_response());
        }
    };

    Ok(MachineHealthSnapshot {
        aggregate_health: machine
            .as_ref()
            .and_then(|m| m.health.as_ref())
            .map(|health| health_report_from_rpc_convert_invalid(health.clone())),
        associated_dpu_machine_ids: machine
            .map(|m| m.associated_dpu_machine_ids)
            .unwrap_or_default(),
    })
}

async fn fetch_rack_aggregate_health(
    api: &Api,
    rack_id: &RackId,
) -> Result<Option<HealthReport>, Response> {
    let rack = match api
        .find_racks_by_ids(tonic::Request::new(RacksByIdsRequest {
            rack_ids: vec![rack_id.clone()],
        }))
        .await
        .map(|response| response.into_inner())
    {
        Ok(r) if r.racks.is_empty() => None,
        Ok(r) if r.racks.len() != 1 => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Rack list for {rack_id} returned {} racks", r.racks.len()),
            )
                .into_response());
        }
        Ok(mut r) => Some(r.racks.remove(0)),
        Err(err) if err.code() == tonic::Code::NotFound => None,
        Err(err) => {
            tracing::error!(%err, %rack_id, "find_racks_by_ids");
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Html(err.to_string())).into_response());
        }
    };

    Ok(rack
        .as_ref()
        .and_then(|rack| rack.status.as_ref())
        .and_then(|status| status.health.as_ref())
        .map(|health| health_report_from_rpc_convert_invalid(health.clone())))
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct HealthReportEntry {
    mode: String,
    health_report: HealthReport,
}

impl HealthReportEntry {
    fn from_rpc_convert_invalid(o: ::rpc::forge::HealthReportEntry) -> Self {
        let mode = match o.mode() {
            HealthReportApplyMode::Merge => "Merge",
            HealthReportApplyMode::Replace => "Replace",
        }
        .to_string();

        let health_report = o
            .report
            .map(health_report_from_rpc_convert_invalid)
            .unwrap_or_else(HealthReport::missing_report);

        HealthReportEntry {
            mode,
            health_report,
        }
    }
}

impl TryFrom<HealthReportEntry> for ::rpc::forge::HealthReportEntry {
    type Error = String;

    fn try_from(value: HealthReportEntry) -> Result<Self, Self::Error> {
        let mode = match value.mode.as_str() {
            "Replace" => HealthReportApplyMode::Replace,
            "Merge" => HealthReportApplyMode::Merge,
            m => {
                return Err(format!(
                    "Apply mode must be \"Replace\" or \"Merge\", but was \"{m}\""
                ));
            }
        };

        Ok(::rpc::forge::HealthReportEntry {
            mode: mode as i32,
            report: Some(::rpc::health::HealthReport::from(value.health_report)),
        })
    }
}

#[derive(serde::Deserialize)]
pub struct RemoveHealthReport {
    source: String,
}

pub async fn add_machine_health_report(
    AxumState(state): AxumState<Arc<Api>>,
    AxumPath(machine_id): AxumPath<String>,
    auth_context: Option<axum::Extension<AuthContext>>,
    extract::Json(payload): extract::Json<HealthReportEntry>,
) -> Response {
    let machine_id = match machine_id.parse::<MachineId>() {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    add_health_report_for(
        state,
        HealthObject::Machine(machine_id),
        auth_context,
        payload,
    )
    .await
}

pub async fn add_rack_health_report(
    AxumState(state): AxumState<Arc<Api>>,
    AxumPath(rack_id): AxumPath<String>,
    auth_context: Option<axum::Extension<AuthContext>>,
    extract::Json(payload): extract::Json<HealthReportEntry>,
) -> Response {
    let rack_id = match rack_id.parse::<RackId>() {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    add_health_report_for(state, HealthObject::Rack(rack_id), auth_context, payload).await
}

async fn add_health_report_for(
    state: Arc<Api>,
    object: HealthObject,
    auth_context: Option<axum::Extension<AuthContext>>,
    payload: HealthReportEntry,
) -> Response {
    let entry = match ::rpc::forge::HealthReportEntry::try_from(payload) {
        Ok(entry) => entry,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    let object_id = object.id();
    let object_kind = object.kind();
    let result = match object {
        HealthObject::Machine(machine_id) => {
            let mut request = tonic::Request::new(InsertMachineHealthReportRequest {
                machine_id: Some(machine_id),
                health_report_entry: Some(entry),
            });
            if let Some(axum::Extension(auth_context)) = auth_context {
                request.extensions_mut().insert(auth_context);
            }
            state
                .insert_machine_health_report(request)
                .await
                .map(|response| response.into_inner())
        }
        HealthObject::Rack(rack_id) => {
            let mut request = tonic::Request::new(InsertRackHealthReportRequest {
                rack_id: Some(rack_id),
                health_report_entry: Some(entry),
            });
            if let Some(axum::Extension(auth_context)) = auth_context {
                request.extensions_mut().insert(auth_context);
            }
            state
                .insert_rack_health_report(request)
                .await
                .map(|response| response.into_inner())
        }
    };

    match result {
        Err(err) if err.code() == tonic::Code::NotFound => {
            (StatusCode::NOT_FOUND, format!("Not found: {object_id}")).into_response()
        }
        Err(err) => {
            tracing::error!(%err, %object_id, object_kind, "insert_health_report");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
        Ok(_) => (StatusCode::OK, String::new()).into_response(),
    }
}

pub async fn remove_machine_health_report(
    AxumState(state): AxumState<Arc<Api>>,
    AxumPath(machine_id): AxumPath<String>,
    extract::Json(payload): extract::Json<RemoveHealthReport>,
) -> Response {
    let machine_id = match machine_id.parse::<MachineId>() {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    remove_health_report_for(state, HealthObject::Machine(machine_id), payload).await
}

pub async fn remove_rack_health_report(
    AxumState(state): AxumState<Arc<Api>>,
    AxumPath(rack_id): AxumPath<String>,
    extract::Json(payload): extract::Json<RemoveHealthReport>,
) -> Response {
    let rack_id = match rack_id.parse::<RackId>() {
        Ok(id) => id,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };

    remove_health_report_for(state, HealthObject::Rack(rack_id), payload).await
}

async fn remove_health_report_for(
    state: Arc<Api>,
    object: HealthObject,
    payload: RemoveHealthReport,
) -> Response {
    let object_id = object.id();
    let object_kind = object.kind();
    let result = match object {
        HealthObject::Machine(machine_id) => state
            .remove_machine_health_report(tonic::Request::new(RemoveMachineHealthReportRequest {
                machine_id: Some(machine_id),
                source: payload.source,
            }))
            .await
            .map(|response| response.into_inner()),
        HealthObject::Rack(rack_id) => state
            .remove_rack_health_report(tonic::Request::new(RemoveRackHealthReportRequest {
                rack_id: Some(rack_id),
                source: payload.source,
            }))
            .await
            .map(|response| response.into_inner()),
    };

    match result {
        Err(err) if err.code() == tonic::Code::NotFound => {
            (StatusCode::NOT_FOUND, format!("Not found: {object_id}")).into_response()
        }
        Err(err) => {
            tracing::error!(%err, %object_id, object_kind, "remove_health_report");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()).into_response()
        }
        Ok(_) => (StatusCode::OK, String::new()).into_response(),
    }
}

fn health_report_from_rpc_convert_invalid(
    report: rpc::health::HealthReport,
) -> health_report::HealthReport {
    health_report::HealthReport::try_from(report)
        .unwrap_or_else(health_report::HealthReport::malformed_report)
}

pub(super) async fn fetch_health_history(
    api: &Api,
    machine_id: &MachineId,
) -> Result<Vec<HealthHistoryRecord>, tonic::Status> {
    let records = api
        .find_machine_health_histories(tonic::Request::new(
            ::rpc::forge::MachineHealthHistoriesRequest {
                machine_ids: vec![*machine_id],
                start_time: None,
                end_time: None,
            },
        ))
        .await
        .map(|response| response.into_inner())?
        .histories
        .remove(&machine_id.to_string())
        .unwrap_or_default()
        .records;

    let records = records
        .into_iter()
        .map(HealthHistoryRecord::from_rpc_convert_invalid)
        .collect();

    Ok(records)
}

impl super::Base for HealthPage {}
