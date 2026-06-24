use std::path::PathBuf;
use clap::Parser;
use oxy_core::Role;

#[derive(Parser)]
#[command(name = "oxydactylus", version, about = "Game server management panel")]
struct Cli {
    #[arg(short = 'c', long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let raw = std::fs::read_to_string(&cli.config)
        .map_err(|e| anyhow::anyhow!("cannot read {:?}: {}", cli.config, e))?;

    let config: oxy_core::Config = toml::from_str(&raw)
        .map_err(|e| anyhow::anyhow!("invalid config.toml: {}", e))?;

    match config.role.kind {
        Role::Panel => {
            let cfg = config.panel
                .ok_or_else(|| anyhow::anyhow!("[panel] section required when role = \"panel\""))?;
            oxy_panel::run(cfg).await?;
        }
        Role::Node => {
            let cfg = config.node
                .ok_or_else(|| anyhow::anyhow!("[node] section required when role = \"node\""))?;
            oxy_node::run(cfg).await?;
        }
        Role::Both => {
            let panel_cfg = config.panel
                .ok_or_else(|| anyhow::anyhow!("[panel] section required when role = \"both\""))?;
            let node_cfg = config.node
                .ok_or_else(|| anyhow::anyhow!("[node] section required when role = \"both\""))?;
            let (p, n) = tokio::join!(oxy_panel::run(panel_cfg), oxy_node::run(node_cfg));
            p?;
            n?;
        }
    }

    Ok(())
}
