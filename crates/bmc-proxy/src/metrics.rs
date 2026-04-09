use std::net::SocketAddr;

use metrics_endpoint::{MetricsEndpointConfig, MetricsSetup};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub async fn start_metrics(
    address: SocketAddr,
    metrics_setup: MetricsSetup,
    cancellation_token: CancellationToken,
    join_set: &mut JoinSet<()>,
) {
    join_set
        .build_task()
        .name("bmc-proxy metrics service")
        .spawn(async move {
            metrics_endpoint::run_metrics_endpoint(
                &MetricsEndpointConfig {
                    address,
                    registry: metrics_setup.registry,
                    health_controller: Some(metrics_setup.health_controller),
                },
                cancellation_token,
            )
            .await
            // Safety: We want this to cause a crash if metrics fails
            .expect("Error running metrics endpoint");
        })
        // Safety: Should only fail if not in a tokio runtime
        .expect("Error spawning metrics endpoint");
}
