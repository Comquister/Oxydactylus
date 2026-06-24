use oxy_core::{NodeConfig, Result};

pub async fn run(config: NodeConfig) -> Result<()> {
    tracing::info!(listen = %config.grpc_listen, "node starting");
    std::future::pending::<()>().await;
    Ok(())
}
