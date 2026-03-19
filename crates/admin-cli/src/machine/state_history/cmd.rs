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
use std::pin::Pin;

use ::rpc::admin_cli::{CarbideCliError, CarbideCliResult, OutputFormat};
use ::rpc::forge::MachineEvent;

use super::args::Args;
use crate::rpc::ApiClient;
use crate::{async_writeln};

pub async fn state_history(
    args: Args,
    output_format: &OutputFormat,
    output_file: &mut Pin<Box<dyn tokio::io::AsyncWrite>>,
    api_client: &ApiClient,
) -> CarbideCliResult<()> {
    let request = ::rpc::forge::MachineStateHistoriesRequest {
        machine_ids: args.machines.clone(),
    };

    let mut histories = api_client
        .0
        .find_machine_state_histories(request)
        .await?
        .histories;

    match output_format {
        OutputFormat::Json => {
            let result: HashMap<String, Vec<MachineEvent>> = args
                .machines
                .iter()
                .map(|id| {
                    let key = id.to_string();
                    let mut records = histories.remove(&key).unwrap_or_default().records;
                    records.reverse();
                    (key, records)
                })
                .collect();
            async_writeln!(output_file, "{}", serde_json::to_string_pretty(&result)?)?;
        }
        OutputFormat::AsciiTable => {
            for machine_id in &args.machines {
                let key = machine_id.to_string();
                let mut records = histories.remove(&key).unwrap_or_default().records;
                records.reverse();

                async_writeln!(output_file, "Machine: {machine_id}")?;
                if records.is_empty() {
                    async_writeln!(output_file, "  (no history)")?;
                } else {
                    let max_state_len =
                        records.iter().map(|r| r.event.len()).max().unwrap_or(0).max("State".len());
                    let max_version_len = records
                        .iter()
                        .map(|r| r.version.len())
                        .max()
                        .unwrap_or(0)
                        .max("Version".len());
                    async_writeln!(
                        output_file,
                        "  {:<max_state_len$} {:<max_version_len$} Time",
                        "State",
                        "Version"
                    )?;
                    for record in &records {
                        async_writeln!(
                            output_file,
                            "  {:<max_state_len$} {:<max_version_len$} {}",
                            record.event,
                            record.version,
                            record.time.unwrap_or_default()
                        )?;
                    }
                }
                async_writeln!(output_file)?;
            }
        }
        OutputFormat::Csv => {
            return Err(CarbideCliError::NotImplemented(
                "CSV formatted output".to_string(),
            ));
        }
        OutputFormat::Yaml => {
            return Err(CarbideCliError::NotImplemented(
                "YAML formatted output".to_string(),
            ));
        }
    }

    Ok(())
}
