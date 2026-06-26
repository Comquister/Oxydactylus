use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use clap::{Parser, Subcommand};
use oxy_core::Role;

#[derive(Parser)]
#[command(name = "oxydactylus", version, about = "Game server management panel")]
struct Cli {
    #[arg(short = 'c', long, default_value = "config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    #[command(subcommand, about = "Manage users")]
    User(UserCmd),
}

#[derive(Subcommand)]
enum UserCmd {
    #[command(about = "Create a new user interactively")]
    Create,
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

    if let Some(Commands::User(UserCmd::Create)) = cli.command {
        let panel_cfg = config.panel
            .ok_or_else(|| anyhow::anyhow!("[panel] section required for user management"))?;
        return user_create(&panel_cfg.database_url).await;
    }

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

async fn user_create(database_url: &str) -> anyhow::Result<()> {
    let email = prompt("Email")?;
    if email.is_empty() {
        anyhow::bail!("email cannot be empty");
    }

    let password = rpassword::prompt_password("Password: ")?;
    if password.is_empty() {
        anyhow::bail!("password cannot be empty");
    }

    let admin_raw = prompt("Is admin? [y/N]")?;
    let is_admin = matches!(admin_raw.to_lowercase().as_str(), "y" | "yes");

    let hash = oxy_panel::auth::hash_password(&password)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {}", e))?;

    let pool = sqlx::PgPool::connect(database_url).await
        .map_err(|e| anyhow::anyhow!("cannot connect to database: {}", e))?;

    sqlx::query("INSERT INTO users (email, password_hash, is_admin) VALUES ($1, $2, $3)")
        .bind(&email)
        .bind(&hash)
        .bind(is_admin)
        .execute(&pool)
        .await
        .map_err(|e| {
            if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
                anyhow::anyhow!("user '{}' already exists", email)
            } else {
                anyhow::anyhow!("database error: {}", e)
            }
        })?;

    println!("User '{}' created (admin: {}).", email, is_admin);
    Ok(())
}

fn prompt(label: &str) -> anyhow::Result<String> {
    print!("{}: ", label);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}
