pub mod docker;
pub mod error;
pub mod interceptor;
pub mod server;
pub mod stream;

use oxy_core::NodeConfig;

pub async fn run(config: NodeConfig) -> oxy_core::Result<()> {
    tracing::info!(listen = %config.grpc_listen, "node starting");
    std::future::pending::<()>().await;
    Ok(())
}
