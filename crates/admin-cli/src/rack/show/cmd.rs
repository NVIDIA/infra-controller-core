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

use color_eyre::Result;
use prettytable::{Table, row};
use rpc::admin_cli::OutputFormat;
use rpc::forge::Rack;
use serde::Serialize;

use super::args::Args;
use crate::cfg::runtime::RuntimeConfig;
use crate::rpc::ApiClient;

#[derive(Serialize)]
struct RackOutput {
    id: String,
    name: String,
    state: String,
    version: String,
    current_compute_trays: Vec<String>,
    current_power_shelves: Vec<String>,
    current_nvlink_switches: Vec<String>,
}

impl From<&Rack> for RackOutput {
    fn from(r: &Rack) -> Self {
        println!("CALEB_DEBUG: r: {:?}", r);
        Self {
            id: r.id.as_ref().map(|id| id.to_string()).unwrap_or_default(),
            name: r
                .metadata
                .as_ref()
                .map(|m| m.name.clone())
                .unwrap_or_default(),
            state: r.rack_state.clone(),
            version: r.version.clone(),
            current_compute_trays: vec![], // TODO: Use API to get current compute trays
            current_power_shelves: vec![], // TODO: Use API to get current power shelves
            current_nvlink_switches: vec![], // TODO: Use API to get current nvlink switches
        }
    }
}

pub async fn show_rack(api_client: &ApiClient, args: Args, config: &RuntimeConfig) -> Result<()> {
    let format = config.format;
    match args.rack {
        Some(rack_id) => {
            let racks = api_client.get_one_rack(rack_id).await?.racks;
            match racks.first() {
                Some(r) => show_single(r, format)?,
                None => println!("No rack found"),
            }
        }
        None => {
            let racks = api_client.get_all_racks(config.page_size).await?.racks;
            if racks.is_empty() {
                println!("No racks found");
            } else {
                show_list(&racks, format)?;
            }
        }
    }

    Ok(())
}

fn show_single(r: &Rack, format: OutputFormat) -> Result<()> {
    let output = RackOutput::from(r);
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&output)?),
        OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&output)?),
        _ => show_detail(&output),
    }
    Ok(())
}

fn show_list(racks: &[Rack], format: OutputFormat) -> Result<()> {
    let outputs: Vec<RackOutput> = racks.iter().map(RackOutput::from).collect();
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&outputs)?),
        OutputFormat::Yaml => println!("{}", serde_yaml::to_string(&outputs)?),
        OutputFormat::Csv => {
            show_table_csv(&outputs);
        }
        _ => show_table(&outputs),
    }
    Ok(())
}

fn show_detail(output: &RackOutput) {
    let mut table = Table::new();
    table.add_row(row!["ID", output.id]);
    table.add_row(row!["Name", output.name]);
    table.add_row(row!["State", output.state]);
    table.add_row(row!["Version", output.version]);
    table.add_row(row![
        "Current Compute Trays",
        if output.current_compute_trays.is_empty() {
            "N/A".to_string()
        } else {
            output
                .current_compute_trays
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        }
    ]);
    table.add_row(row![
        "Current Power Shelves",
        if output.current_power_shelves.is_empty() {
            "N/A".to_string()
        } else {
            output
                .current_power_shelves
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        }
    ]);
    table.add_row(row![
        "Current NVLink Switches",
        if output.current_nvlink_switches.is_empty() {
            "N/A".to_string()
        } else {
            output
                .current_nvlink_switches
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        }
    ]);
    table.printstd();
}

fn show_table(outputs: &[RackOutput]) {
    let mut table = Table::new();
    table.set_titles(row![
        "ID",
        "Name",
        "State",
        "Compute Trays",
        "Power Shelves",
        "Switches",
    ]);

    for output in outputs {
        table.add_row(row![
            output.id,
            output.name,
            output.state,
            format!("{}", output.current_compute_trays.len(),),
            format!("{}", output.current_power_shelves.len(),),
            format!("{}", output.current_nvlink_switches.len(),),
        ]);
    }

    table.printstd();
}

fn show_table_csv(outputs: &[RackOutput]) {
    let mut table = Table::new();
    table.set_titles(row![
        "ID",
        "Name",
        "State",
        "Compute Trays",
        "Power Shelves",
        "Switches",
    ]);

    for output in outputs {
        table.add_row(row![
            output.id,
            output.name,
            output.state,
            format!("{}", output.current_compute_trays.len(),),
            format!("{}", output.current_power_shelves.len(),),
            format!("{}", output.current_nvlink_switches.len(),),
        ]);
    }

    table.to_csv(std::io::stdout()).ok();
}

#[cfg(test)]
mod tests {

    use rpc::forge::{Metadata, Rack};
    use super::*;

    fn make_rack(id: &str, state: &str, name: &str, version: &str) -> Rack {
        Rack {
            id: Some(id.parse().unwrap()),
            rack_state: state.to_string(),
            version: version.to_string(),
            metadata: Some(Metadata {
                name: name.to_string(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Test that the RackOutput maps the fields from the Rack correctly,
    /// and the current compute trays, power shelves, and nvlink switches are empty.
    #[test]
    fn rack_output_maps_fields_from_rack() {
        let id = "Rack1";
        let rack_state = "Created";
        let metadata_name = "NVL72";
        let version= "V1-T1777407111818648";
        let rack = make_rack(id, rack_state, metadata_name, version);
        let output = RackOutput::from(&rack);
        assert_eq!(output.id, id);
        assert_eq!(output.name, metadata_name);
        assert_eq!(output.state, rack_state);
        assert_eq!(output.version, version);

        assert!(output.current_compute_trays.is_empty());
        assert!(output.current_power_shelves.is_empty());
        assert!(output.current_nvlink_switches.is_empty());
    }
}