use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    db,
    error::{PanelError, Result},
    servers::{check_server_access, fetch_server},
    AppState,
};

fn validate_mysql_username(username: &str) -> Result<()> {
    if username.is_empty() || username.len() > 32 {
        return Err(PanelError::Validation(
            "MySQL username must be 1-32 characters".to_string(),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(PanelError::Validation(
            "MySQL username must contain only alphanumeric characters and underscores".to_string(),
        ));
    }
    Ok(())
}

fn validate_mysql_database_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(PanelError::Validation(
            "database name must be 1-64 characters".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(PanelError::Validation(
            "database name must contain only alphanumeric characters, underscores, and hyphens".to_string(),
        ));
    }
    Ok(())
}

fn validate_mysql_remote_host(remote: &str) -> Result<()> {
    if remote.is_empty() || remote.len() > 255 {
        return Err(PanelError::Validation(
            "remote host must be 1-255 characters".to_string(),
        ));
    }

    if remote == "%" || remote == "localhost" {
        return Ok(());
    }

    let domain_pattern = Regex::new(r"^[a-zA-Z0-9%]([a-zA-Z0-9\-\.%]{0,253}[a-zA-Z0-9%])?$").unwrap();
    if domain_pattern.is_match(remote) {
        return Ok(());
    }

    Err(PanelError::Validation(
        "invalid remote host format: must be '%', 'localhost', IP address, or domain name".to_string(),
    ))
}

fn escape_mysql_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[derive(Debug, Serialize)]
pub struct ServerDatabase {
    pub id: Uuid,
    pub server_id: Uuid,
    pub host_id: Uuid,
    pub database_name: String,
    pub username: String,
    pub remote: String,
    #[serde(skip)]
    pub password: String,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for ServerDatabase {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let server_id_str: String = row.try_get("server_id")?;
        let server_id = Uuid::parse_str(&server_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let host_id_str: String = row.try_get("host_id")?;
        let host_id = Uuid::parse_str(&host_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            server_id,
            host_id,
            database_name: row.try_get("database_name")?,
            username: row.try_get("username")?,
            remote: row.try_get("remote")?,
            password: row.try_get("password")?,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateServerDatabaseRequest {
    host_id: Uuid,
    database_name: String,
    username: Option<String>,
    remote: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseHostInfo {
    pub host: String,
    pub port: i32,
    pub username: String,
    #[serde(skip)]
    pub password: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for DatabaseHostInfo {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self {
            host: row.try_get("host")?,
            port: row.try_get("port")?,
            username: row.try_get("username")?,
            password: row.try_get("password")?,
        })
    }
}

async fn list_server_databases(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<ServerDatabase>>> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some("database.read"), &state.db).await?;

    let sql = db::port_sql(
        "SELECT id, server_id, host_id, database_name, username, remote, password, created_at FROM server_databases WHERE server_id = $1 ORDER BY created_at",
        &state.db_backend,
    );
    let databases = sqlx::query_as::<_, ServerDatabase>(&sql)
        .bind(server_id.to_string())
        .fetch_all(&state.db)
        .await?;

    Ok(Json(databases))
}

async fn create_server_database(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<CreateServerDatabaseRequest>,
) -> Result<(StatusCode, Json<ServerDatabase>)> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some("database.create"), &state.db).await?;

    validate_mysql_database_name(&body.database_name)?;

    let username = body.username.unwrap_or_else(|| format!("db_{}", Uuid::new_v4().to_string()[..8].to_string()));
    let remote = body.remote.unwrap_or_else(|| "%".to_string());

    validate_mysql_username(&username)?;
    validate_mysql_remote_host(&remote)?;

    let host_sql = db::port_sql(
        "SELECT host, port, username, password FROM database_hosts WHERE id = $1",
        &state.db_backend,
    );
    let host_info = sqlx::query_as::<_, DatabaseHostInfo>(&host_sql)
        .bind(body.host_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound(format!("database host {}", body.host_id)))?;

    let db_password = match &state.app_key {
        Some(key) => {
            let decrypted = db::decrypt_password(&host_info.password, key)?;
            decrypted
        }
        None => host_info.password,
    };

    let mysql_url = format!(
        "mysql://{}:{}@{}:{}",
        host_info.username, db_password, host_info.host, host_info.port
    );

    let mysql_pool = sqlx::MySqlPool::connect(&mysql_url)
        .await
        .map_err(|e| PanelError::Internal(format!("failed to connect to MySQL host: {}", e)))?;

    let db_name = &body.database_name;
    let db_user = &username;
    let db_pass = Uuid::new_v4().to_string();
    let db_remote = &remote;

    sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS `{}`", db_name.replace('`', "")))
        .execute(&mysql_pool)
        .await
        .map_err(|e| PanelError::Internal(format!("failed to create database: {}", e)))?;

    let escaped_pass = escape_mysql_string(&db_pass);
    let escaped_remote = escape_mysql_string(&db_remote);

    sqlx::query(&format!(
        "CREATE USER IF NOT EXISTS '{}'@'{}' IDENTIFIED BY '{}'",
        db_user, escaped_remote, escaped_pass
    ))
    .execute(&mysql_pool)
    .await
    .map_err(|e| PanelError::Internal(format!("failed to create database user: {}", e)))?;

    sqlx::query(&format!(
        "GRANT ALL PRIVILEGES ON `{}`.* TO '{}'@'{}'",
        db_name.replace('`', ""), db_user, escaped_remote
    ))
    .execute(&mysql_pool)
    .await
    .map_err(|e| PanelError::Internal(format!("failed to grant privileges: {}", e)))?;

    sqlx::query("FLUSH PRIVILEGES")
        .execute(&mysql_pool)
        .await
        .map_err(|e| PanelError::Internal(format!("failed to flush privileges: {}", e)))?;

    let encrypted_pass = match &state.app_key {
        Some(key) => db::encrypt_password(&db_pass, key)?,
        None => {
            tracing::warn!("app_key not configured: storing database password unencrypted");
            db_pass.clone()
        }
    };

    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let sql = db::port_sql(
        "INSERT INTO server_databases (id, server_id, host_id, database_name, username, remote, password, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, server_id, host_id, database_name, username, remote, password, created_at",
        &state.db_backend,
    );
    let database = sqlx::query_as::<_, ServerDatabase>(&sql)
        .bind(&id)
        .bind(server_id.to_string())
        .bind(body.host_id.to_string())
        .bind(&body.database_name)
        .bind(&username)
        .bind(&remote)
        .bind(&encrypted_pass)
        .bind(&created_at)
        .fetch_one(&state.db)
        .await?;

    crate::activity::log_activity(
        state.db.clone(),
        state.db_backend.clone(),
        crate::activity::ActivityEntry {
            server_id: Some(server_id),
            user_id: Some(user.id),
            event: "database.created".to_string(),
            properties: serde_json::json!({
                "database_id": database.id,
                "database_name": body.database_name,
                "username": username,
                "host_id": body.host_id
            }),
            ip: None,
        },
    );

    Ok((StatusCode::CREATED, Json(database)))
}

async fn delete_server_database(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, database_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some("database.delete"), &state.db).await?;

    let db_sql = db::port_sql(
        "SELECT host_id, database_name, username, remote FROM server_databases WHERE id = $1 AND server_id = $2",
        &state.db_backend,
    );
    let db_info: (String, String, String, String) = sqlx::query_as(&db_sql)
        .bind(database_id.to_string())
        .bind(server_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound(database_id.to_string()))?;

    let (host_id_str, db_name, db_user, db_remote) = db_info;

    let host_sql = db::port_sql(
        "SELECT host, port, username, password FROM database_hosts WHERE id = $1",
        &state.db_backend,
    );
    let host_info = sqlx::query_as::<_, DatabaseHostInfo>(&host_sql)
        .bind(&host_id_str)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound(format!("database host {}", host_id_str)))?;

    let db_password = match &state.app_key {
        Some(key) => db::decrypt_password(&host_info.password, key)?,
        None => host_info.password,
    };

    let mysql_url = format!(
        "mysql://{}:{}@{}:{}",
        host_info.username, db_password, host_info.host, host_info.port
    );

    let mysql_pool = sqlx::MySqlPool::connect(&mysql_url)
        .await
        .map_err(|e| PanelError::Internal(format!("failed to connect to MySQL host: {}", e)))?;

    let escaped_remote = escape_mysql_string(&db_remote);
    sqlx::query(&format!(
        "DROP USER IF EXISTS '{}'@'{}'",
        db_user, escaped_remote
    ))
    .execute(&mysql_pool)
    .await
    .ok();

    sqlx::query(&format!("DROP DATABASE IF NOT EXISTS `{}`", db_name.replace('`', "")))
        .execute(&mysql_pool)
        .await
        .ok();

    sqlx::query("FLUSH PRIVILEGES")
        .execute(&mysql_pool)
        .await
        .ok();

    let del_sql = db::port_sql(
        "DELETE FROM server_databases WHERE id = $1",
        &state.db_backend,
    );
    sqlx::query(&del_sql)
        .bind(database_id.to_string())
        .execute(&state.db)
        .await?;

    crate::activity::log_activity(
        state.db.clone(),
        state.db_backend.clone(),
        crate::activity::ActivityEntry {
            server_id: Some(server_id),
            user_id: Some(user.id),
            event: "database.deleted".to_string(),
            properties: serde_json::json!({
                "database_id": database_id,
                "database_name": db_name,
                "username": db_user
            }),
            ip: None,
        },
    );

    Ok(StatusCode::NO_CONTENT)
}

async fn rotate_database_password(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, database_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ServerDatabase>> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some("database.update"), &state.db).await?;

    let db_sql = db::port_sql(
        "SELECT host_id, database_name, username, remote FROM server_databases WHERE id = $1 AND server_id = $2",
        &state.db_backend,
    );
    let db_info: (String, String, String, String) = sqlx::query_as(&db_sql)
        .bind(database_id.to_string())
        .bind(server_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound(database_id.to_string()))?;

    let (host_id_str, db_name, db_user, db_remote) = db_info;

    let host_sql = db::port_sql(
        "SELECT host, port, username, password FROM database_hosts WHERE id = $1",
        &state.db_backend,
    );
    let host_info = sqlx::query_as::<_, DatabaseHostInfo>(&host_sql)
        .bind(&host_id_str)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound(format!("database host {}", host_id_str)))?;

    let db_password = match &state.app_key {
        Some(key) => db::decrypt_password(&host_info.password, key)?,
        None => host_info.password,
    };

    let mysql_url = format!(
        "mysql://{}:{}@{}:{}",
        host_info.username, db_password, host_info.host, host_info.port
    );

    let mysql_pool = sqlx::MySqlPool::connect(&mysql_url)
        .await
        .map_err(|e| PanelError::Internal(format!("failed to connect to MySQL host: {}", e)))?;

    let new_pass = Uuid::new_v4().to_string();
    let escaped_pass = escape_mysql_string(&new_pass);
    let escaped_remote = escape_mysql_string(&db_remote);

    sqlx::query(&format!(
        "ALTER USER '{}'@'{}' IDENTIFIED BY '{}'",
        db_user, escaped_remote, escaped_pass
    ))
    .execute(&mysql_pool)
    .await
    .map_err(|e| PanelError::Internal(format!("failed to rotate password: {}", e)))?;

    sqlx::query("FLUSH PRIVILEGES")
        .execute(&mysql_pool)
        .await
        .ok();

    let encrypted_pass = match &state.app_key {
        Some(key) => db::encrypt_password(&new_pass, key)?,
        None => {
            tracing::warn!("app_key not configured: storing database password unencrypted");
            new_pass
        }
    };

    let update_sql = db::port_sql(
        "UPDATE server_databases SET password = $1 WHERE id = $2 RETURNING id, server_id, host_id, database_name, username, remote, password, created_at",
        &state.db_backend,
    );
    let database = sqlx::query_as::<_, ServerDatabase>(&update_sql)
        .bind(&encrypted_pass)
        .bind(database_id.to_string())
        .fetch_one(&state.db)
        .await?;

    crate::activity::log_activity(
        state.db.clone(),
        state.db_backend.clone(),
        crate::activity::ActivityEntry {
            server_id: Some(server_id),
            user_id: Some(user.id),
            event: "database.password_rotated".to_string(),
            properties: serde_json::json!({
                "database_id": database_id,
                "database_name": db_name,
                "username": db_user
            }),
            ip: None,
        },
    );

    Ok(Json(database))
}

pub fn server_databases_router() -> Router<AppState> {
    Router::new()
        .route("/:id/databases", get(list_server_databases).post(create_server_database))
        .route("/:id/databases/:dbid", delete(delete_server_database))
        .route("/:id/databases/:dbid/rotate-password", post(rotate_database_password))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn make_state(pool: sqlx::PgPool) -> AppState {
        use sqlx::ConnectOptions;
        sqlx::any::install_default_drivers();
        let db_url = pool.connect_options().to_url_lossy().to_string();
        let any_pool = sqlx::AnyPool::connect(&db_url).await.unwrap();
        AppState {
            db: any_pool,
            db_backend: "PostgreSQL".to_string(),
            jwt_secret: "test-secret-at-least-32-chars-long!!".to_string(),
            app_key: Some("test-key-that-is-exactly-32-chars!!".to_string()),
        }
    }

    async fn seed_user(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = crate::auth::hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("user@test.com")
        .bind(&hash)
        .bind(false)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        let token = crate::auth::encode_token(
            id,
            false,
            "access",
            "test-secret-at-least-32-chars-long!!",
            900,
        )
        .unwrap();
        (id, token)
    }

    async fn seed_server(pool: &sqlx::PgPool, user_id: Uuid) -> Uuid {
        let id = Uuid::new_v4();
        let node_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(node_id.to_string())
        .bind("test-node")
        .bind("localhost:50051")
        .bind("test-token")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, env, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind("test-server")
        .bind("ubuntu:latest")
        .bind(1024)
        .bind(50)
        .bind("[]")
        .bind("running")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        id
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_server_databases_requires_auth(pool: sqlx::PgPool) {
        let (user_id, _) = seed_user(&pool).await;
        let server_id = seed_server(&pool, user_id).await;

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri(&format!("/api/servers/{}/databases", server_id))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_user_can_list_own_server_databases(pool: sqlx::PgPool) {
        let (user_id, token) = seed_user(&pool).await;
        let server_id = seed_server(&pool, user_id).await;

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri(&format!("/api/servers/{}/databases", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn test_validate_mysql_username_accepts_valid_names() {
        assert!(validate_mysql_username("db_user").is_ok());
        assert!(validate_mysql_username("db_123").is_ok());
        assert!(validate_mysql_username("a").is_ok());
        assert!(validate_mysql_username(&"x".repeat(32)).is_ok());
    }

    #[test]
    fn test_validate_mysql_username_rejects_invalid_names() {
        assert!(validate_mysql_username("").is_err());
        assert!(validate_mysql_username(&"x".repeat(33)).is_err());
        assert!(validate_mysql_username("db-user").is_err());
        assert!(validate_mysql_username("db@user").is_err());
        assert!(validate_mysql_username("user name").is_err());
        assert!(validate_mysql_username("db'user").is_err());
    }

    #[test]
    fn test_validate_mysql_database_name_accepts_valid_names() {
        assert!(validate_mysql_database_name("mydb").is_ok());
        assert!(validate_mysql_database_name("my_db").is_ok());
        assert!(validate_mysql_database_name("my-db").is_ok());
        assert!(validate_mysql_database_name("db123").is_ok());
        assert!(validate_mysql_database_name(&"x".repeat(64)).is_ok());
    }

    #[test]
    fn test_validate_mysql_database_name_rejects_invalid_names() {
        assert!(validate_mysql_database_name("").is_err());
        assert!(validate_mysql_database_name(&"x".repeat(65)).is_err());
        assert!(validate_mysql_database_name("my db").is_err());
        assert!(validate_mysql_database_name("my@db").is_err());
        assert!(validate_mysql_database_name("my'db").is_err());
    }

    #[test]
    fn test_validate_mysql_remote_host_accepts_valid_hosts() {
        assert!(validate_mysql_remote_host("%").is_ok());
        assert!(validate_mysql_remote_host("localhost").is_ok());
        assert!(validate_mysql_remote_host("192.168.1.1").is_ok());
        assert!(validate_mysql_remote_host("192.168.%.%").is_ok());
        assert!(validate_mysql_remote_host("example.com").is_ok());
        assert!(validate_mysql_remote_host("db.example.com").is_ok());
    }

    #[test]
    fn test_validate_mysql_remote_host_rejects_invalid_hosts() {
        assert!(validate_mysql_remote_host("").is_err());
        assert!(validate_mysql_remote_host(&"x".repeat(256)).is_err());
    }

    #[test]
    fn test_escape_mysql_string_escapes_backslash() {
        assert_eq!(escape_mysql_string("test\\pass"), "test\\\\pass");
        assert_eq!(
            escape_mysql_string("pass\\with\\multiple"),
            "pass\\\\with\\\\multiple"
        );
    }

    #[test]
    fn test_escape_mysql_string_escapes_single_quote() {
        assert_eq!(escape_mysql_string("test'pass"), "test\\'pass");
        assert_eq!(escape_mysql_string("'test'"), "\\'test\\'");
    }

    #[test]
    fn test_escape_mysql_string_escapes_both_backslash_and_quote() {
        assert_eq!(escape_mysql_string("test\\'pass"), "test\\\\\\'pass");
        assert_eq!(
            escape_mysql_string("pass\\with'quote"),
            "pass\\\\with\\'quote"
        );
    }

    #[test]
    fn test_escape_mysql_string_does_not_escape_other_chars() {
        assert_eq!(
            escape_mysql_string("normal_password_123"),
            "normal_password_123"
        );
    }
}
