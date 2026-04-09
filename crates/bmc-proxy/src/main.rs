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

use std::sync::Arc;

use carbide_bmc_proxy::bmc_proxy::{BmcProxyError, BmcProxyParams};
use carbide_bmc_proxy::config::ConfigError;
use carbide_bmc_proxy::setup::{SetupError, setup_metrics};
use carbide_bmc_proxy::{Config, setup_logging};
use clap::Parser;
use sqlx::postgres::PgSslMode;
use sqlx::{ConnectOptions, PgPool};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing_log::AsLog;

#[derive(Parser)]
#[clap(name = "carbide-api")]
pub struct Args {
    #[clap(long, default_value = "false", help = "Print version number and exit")]
    pub version: bool,

    #[clap(short, long)]
    pub debug: bool,

    #[clap(long)]
    pub config_path: String,
}

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    #[error("Error setting up bmc-proxy: {0}")]
    Setup(#[from] SetupError),
    #[error("Error running bmc-proxy: {0}")]
    BmcProxy(#[from] BmcProxyError),
    #[error("Error connecting to database: {0}")]
    DatabaseConnection(sqlx::Error),
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse();
    if args.version {
        println!("{}", carbide_version::version!());
        return Ok(());
    }
    let debug = args.debug;
    let config_str = tokio::fs::read_to_string(&args.config_path)
        .await
        .map_err(|e| {
            ConfigError::Read(format!(
                "Error opening config file at {}: {}",
                args.config_path, e
            ))
        })?;
    let config = Config::parse(&config_str)?;

    setup_logging(debug)?;
    let metrics_setup = setup_metrics()?;

    let mut join_set = JoinSet::new();
    let cancel_token = CancellationToken::new();
    let pg_pool = connect_to_database(&config).await?;

    let start_params = BmcProxyParams {
        config: Arc::new(config),
        credential_config: Default::default(),
        meter: metrics_setup.meter.clone(),
        pg_pool,
    };

    let proxy_cancel_token = cancel_token.clone();

    // Cancel things when we get a ctrl+c
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        cancel_token.cancel();
    });

    carbide_bmc_proxy::bmc_proxy::start(start_params, proxy_cancel_token, &mut join_set).await?;
    join_set.join_all().await;

    Ok(())
}

async fn connect_to_database(config: &Config) -> Result<PgPool, Error> {
    // We need logs to be enabled at least at `INFO` level. Otherwise
    // our global logging filter would reject the logs before they get injected
    // into the `SqlxQueryTracing` layer.
    let mut database_connect_options = config
        .database_url
        .parse::<sqlx::postgres::PgConnectOptions>()
        .map_err(|e| ConfigError::DatabaseUrl(e.to_string()))?
        .log_statements(tracing::metadata::Level::INFO.as_log().to_level_filter());
    let tls_disabled = std::env::var("DISABLE_TLS_ENFORCEMENT").is_ok(); // the integration test doesn't like this
    if !tls_disabled {
        tracing::info!("using TLS for postgres connection.");
        database_connect_options = database_connect_options
            .ssl_mode(PgSslMode::VerifyFull)
            .ssl_root_cert(&config.tls.root_cafile_path);
    }
    Ok(sqlx::pool::PoolOptions::new()
        .max_connections(config.max_database_connections)
        .connect_with(database_connect_options)
        .await
        .map_err(Error::DatabaseConnection)?)
}
