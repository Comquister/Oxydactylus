use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AdminUser,
    db,
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct DatabaseHost {
    pub id: Uuid,
    pub node_id: Option<Uuid>,
    pub name: String,
    pub host: String,
    pub port: i32,
    pub username: String,
    #[serde(skip)]
    pub password: String,
    pub max_databases: i32,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for DatabaseHost {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let node_id_opt: Option<String> = row.try_get("node_id")?;
        let node_id = match node_id_opt {
            Some(s) if !s.is_empty() => Some(Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))?),
            _ => None,
        };

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            node_id,
            name: row.try_get("name")?,
            host: row.try_get("host")?,
            port: row.try_get("port")?,
            username: row.try_get("username")?,
            password: row.try_get("password")?,
            max_databases: row.try_get("max_databases")?,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateDatabaseHostRequest {
    node_id: Option<Uuid>,
    name: String,
    host: String,
    port: Option<i32>,
    username: String,
    password: String,
    max_databases: Option<i32>,
}

async fn list_hosts(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<DatabaseHost>>> {
    let sql = db::port_sql(
        "SELECT id, node_id, name, host, port, username, password, max_databases, created_at FROM database_hosts ORDER BY created_at",
        &state.db_backend,
    );
    let hosts = sqlx::query_as::<_, DatabaseHost>(&sql)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(hosts))
}

async fn create_host(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateDatabaseHostRequest>,
) -> Result<(StatusCode, Json<DatabaseHost>)> {
    if body.name.is_empty() || body.host.is_empty() || body.username.is_empty() || body.password.is_empty() {
        return Err(PanelError::Validation(
            "name, host, username, and password are required".to_string(),
        ));
    }

    let port = body.port.unwrap_or(3306);
    let max_databases = body.max_databases.unwrap_or(0);

    let password = match &state.app_key {
        Some(key) => db::encrypt_password(&body.password, key)?,
        None => {
            tracing::warn!("app_key not configured: storing database password unencrypted");
            body.password
        }
    };

    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let sql = db::port_sql(
        "INSERT INTO database_hosts (id, node_id, name, host, port, username, password, max_databases, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, node_id, name, host, port, username, password, max_databases, created_at",
        &state.db_backend,
    );
    let host = sqlx::query_as::<_, DatabaseHost>(&sql)
        .bind(&id)
        .bind(body.node_id.map(|u| u.to_string()))
        .bind(&body.name)
        .bind(&body.host)
        .bind(port)
        .bind(&body.username)
        .bind(&password)
        .bind(max_databases)
        .bind(&created_at)
        .fetch_one(&state.db)
        .await?;

    crate::activity::log_activity(
        state.db.clone(),
        state.db_backend.clone(),
        crate::activity::ActivityEntry {
            server_id: None,
            user_id: None,
            event: "database_host.created".to_string(),
            properties: serde_json::json!({ "host_id": host.id, "host_name": body.name }),
            ip: None,
        },
    );

    Ok((StatusCode::CREATED, Json(host)))
}

async fn delete_host(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let sql = db::port_sql(
        "DELETE FROM database_hosts WHERE id = $1",
        &state.db_backend,
    );
    let rows = sqlx::query(&sql)
        .bind(id.to_string())
        .execute(&state.db)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(PanelError::NotFound(id.to_string()));
    }

    crate::activity::log_activity(
        state.db.clone(),
        state.db_backend.clone(),
        crate::activity::ActivityEntry {
            server_id: None,
            user_id: None,
            event: "database_host.deleted".to_string(),
            properties: serde_json::json!({ "host_id": id }),
            ip: None,
        },
    );

    Ok(StatusCode::NO_CONTENT)
}

pub fn database_hosts_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_hosts).post(create_host))
        .route("/:id", delete(delete_host))
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

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = crate::auth::hash_password("admin-pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("admin@test.com")
        .bind(&hash)
        .bind(true)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();

        let token = crate::auth::encode_token(
            id,
            true,
            "access",
            "test-secret-at-least-32-chars-long!!",
            900,
        )
        .unwrap();
        (id, token)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_create_database_host_stores_encrypted_password(pool: sqlx::PgPool) {
        let (_id, token) = seed_admin(&pool).await;
        let app = router(make_state(pool.clone()).await);

        let body = serde_json::json!({
            "name": "mysql-host-1",
            "host": "mysql.example.com",
            "port": 3306,
            "username": "admin",
            "password": "secretpassword",
            "max_databases": 10
        });

        let req = Request::builder()
            .method("POST")
            .uri("/api/database-hosts")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["name"], "mysql-host-1");
        assert_eq!(json["host"], "mysql.example.com");

        let stored = sqlx::query("SELECT password FROM database_hosts WHERE name = 'mysql-host-1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        use sqlx::Row;
        let stored_password: String = stored.get("password");
        assert!(stored_password.starts_with("enc:"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_list_database_hosts_requires_admin(pool: sqlx::PgPool) {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("user@test.com")
        .bind(crate::auth::hash_password("pass").unwrap())
        .bind(false)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
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

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/database-hosts")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_admin_can_list_database_hosts(pool: sqlx::PgPool) {
        let (_id, token) = seed_admin(&pool).await;
        let app = router(make_state(pool).await);

        let req = Request::builder()
            .method("GET")
            .uri("/api/database-hosts")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}
