use oxy_core::{PanelConfig, Result};

pub async fn run(config: PanelConfig) -> Result<()> {
    tracing::info!(listen = %config.http_listen, "panel starting");
    std::future::pending::<()>().await;
    Ok(())
}
