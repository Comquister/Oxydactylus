use crate::error::{PanelError, Result};
use sqlx::{any::AnyPoolOptions, AnyPool};
use std::borrow::Cow;
use std::sync::OnceLock;
use regex::Regex;

static PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();

pub fn port_sql<'a>(sql: &'a str, backend: &str) -> Cow<'a, str> {
    if backend == "MySQL" {
        let re = PLACEHOLDER_RE.get_or_init(|| Regex::new(r"\$\d+").unwrap());
        Cow::Owned(re.replace_all(sql, "?").into_owned())
    } else {
        Cow::Borrowed(sql)
    }
}

pub async fn create_pool(database_url: &str) -> Result<AnyPool> {
    AnyPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))
}

pub async fn run_migrations(pool: &AnyPool) -> Result<()> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))
}
