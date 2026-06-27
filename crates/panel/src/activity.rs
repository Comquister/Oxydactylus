use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    db::port_sql,
    error::{PanelError, Result},
    servers::fetch_server,
    AppState,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActivityEntry {
    pub server_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub event: String,
    pub properties: serde_json::Value,
    pub ip: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ActivityLog {
    pub id: Uuid,
    pub batch_id: Option<String>,
    pub server_id: Option<Uuid>,
    pub user_id: Option<Uuid>,
    pub event: String,
    pub properties: serde_json::Value,
    pub ip: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for ActivityLog {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let server_id_opt: Option<String> = row.try_get("server_id")?;
        let server_id = match server_id_opt {
            Some(s) if !s.is_empty() => Some(Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))?),
            _ => None,
        };

        let user_id_opt: Option<String> = row.try_get("user_id")?;
        let user_id = match user_id_opt {
            Some(s) if !s.is_empty() => Some(Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))?),
            _ => None,
        };

        let properties_str: String = row.try_get("properties")?;
        let properties: serde_json::Value = serde_json::from_str(&properties_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            batch_id: row.try_get("batch_id")?,
            server_id,
            user_id,
            event: row.try_get("event")?,
            properties,
            ip: row.try_get("ip")?,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    page: Option<i32>,
    per_page: Option<i32>,
}

impl PaginationParams {
    fn page(&self) -> i32 {
        self.page.unwrap_or(1).max(1)
    }

    fn per_page(&self) -> i32 {
        self.per_page.unwrap_or(50).min(100).max(1)
    }

    fn offset(&self) -> i32 {
        (self.page() - 1) * self.per_page()
    }
}

pub async fn log_activity(pool: AnyPool, backend: String, entry: ActivityEntry) {
    tokio::spawn(async move {
        let _ = log_activity_inner(&pool, &backend, entry).await;
    });
}

async fn log_activity_inner(pool: &AnyPool, backend: &str, entry: ActivityEntry) -> Result<()> {
    let id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    let properties_json = serde_json::to_string(&entry.properties)
        .unwrap_or_else(|_| "{}".to_string());

    let sql = port_sql(
        "INSERT INTO activity_logs (id, server_id, user_id, event, properties, ip, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
        backend,
    );

    sqlx::query(&sql)
        .bind(id.to_string())
        .bind(entry.server_id.map(|u| u.to_string()))
        .bind(entry.user_id.map(|u| u.to_string()))
        .bind(&entry.event)
        .bind(&properties_json)
        .bind(&entry.ip)
        .bind(&now)
        .execute(pool)
        .await?;

    Ok(())
}

async fn server_activity(
    State(state): State<AppState>,
    Path(server_id): Path<Uuid>,
    user: AuthUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<ActivityLog>>> {
    let server = fetch_server(&state.db, server_id).await?;
    if !user.is_admin && server.user_id != user.id {
        return Err(PanelError::Forbidden);
    }

    let sql = port_sql(
        "SELECT id, batch_id, server_id, user_id, event, properties, ip, created_at
         FROM activity_logs WHERE server_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        &state.db_backend,
    );

    let offset = params.offset();
    let per_page = params.per_page();

    let logs = sqlx::query_as::<_, ActivityLog>(&sql)
        .bind(server_id.to_string())
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await?;

    Ok(Json(logs))
}

async fn all_activity(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<ActivityLog>>> {
    let sql = port_sql(
        "SELECT id, batch_id, server_id, user_id, event, properties, ip, created_at
         FROM activity_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        &state.db_backend,
    );

    let offset = params.offset();
    let per_page = params.per_page();

    let logs = sqlx::query_as::<_, ActivityLog>(&sql)
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await?;

    Ok(Json(logs))
}

async fn account_activity(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<ActivityLog>>> {
    let sql = port_sql(
        "SELECT id, batch_id, server_id, user_id, event, properties, ip, created_at
         FROM activity_logs WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        &state.db_backend,
    );

    let offset = params.offset();
    let per_page = params.per_page();

    let logs = sqlx::query_as::<_, ActivityLog>(&sql)
        .bind(user.id.to_string())
        .bind(per_page as i64)
        .bind(offset as i64)
        .fetch_all(&state.db)
        .await?;

    Ok(Json(logs))
}

pub fn activity_router() -> Router<AppState> {
    Router::new()
        .route("/api/servers/:id/activity", get(server_activity))
        .route("/api/activity", get(all_activity))
        .route("/api/account/activity", get(account_activity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use sqlx::{ConnectOptions, PgPool};
    use tower::ServiceExt;

    async fn make_state(pool: PgPool) -> AppState {
        sqlx::any::install_default_drivers();
        let db_url = pool.connect_options().to_url_lossy().to_string();
        let any_pool = sqlx::AnyPool::connect(&db_url).await.unwrap();
        AppState {
            db: any_pool,
            db_backend: "PostgreSQL".to_string(),
            jwt_secret: "secret".to_string(),
            app_key: None,
        }
    }

    fn make_router(state: AppState) -> axum::Router {
        axum::Router::new()
            .route("/api/servers/:id/activity", get(server_activity))
            .route("/api/activity", get(all_activity))
            .route("/api/account/activity", get(account_activity))
            .with_state(state)
    }

    async fn seed_user(pool: &PgPool) -> (Uuid, String) {
        use crate::auth::encode_token;
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)")
            .bind(user_id.to_string())
            .bind(format!("user{}@test.com", Uuid::new_v4()))
            .bind("hash")
            .bind(false)
            .execute(pool)
            .await
            .unwrap();
        let token = encode_token(user_id, false, "access", "secret", 3600).unwrap();
        (user_id, token)
    }

    async fn seed_admin(pool: &PgPool) -> (Uuid, String) {
        use crate::auth::encode_token;
        let user_id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)")
            .bind(user_id.to_string())
            .bind(format!("admin{}@test.com", Uuid::new_v4()))
            .bind("hash")
            .bind(true)
            .execute(pool)
            .await
            .unwrap();
        let token = encode_token(user_id, true, "access", "secret", 3600).unwrap();
        (user_id, token)
    }

    async fn seed_server(pool: &PgPool, user_id: Uuid) -> Uuid {
        let node_id = Uuid::new_v4();
        sqlx::query("INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(node_id.to_string())
            .bind(format!("node{}", Uuid::new_v4()))
            .bind("localhost:50051")
            .bind("token")
            .bind(Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .unwrap();

        let server_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
            .bind(server_id.to_string())
            .bind(user_id.to_string())
            .bind(node_id.to_string())
            .bind(format!("server{}", Uuid::new_v4()))
            .bind("ubuntu:latest")
            .bind(1024)
            .bind(50)
            .bind("running")
            .bind(Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .unwrap();

        server_id
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_log_activity_does_not_block_on_error(pool: sqlx::PgPool) {
        let (user_id, _token) = seed_user(&pool).await;
        let state = make_state(pool).await;

        let entry = ActivityEntry {
            server_id: None,
            user_id: Some(user_id),
            event: "test.event".to_string(),
            properties: serde_json::json!({}),
            ip: None,
        };

        let start = std::time::Instant::now();
        log_activity(state.db.clone(), "PostgreSQL".to_string(), entry).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed.as_millis() < 100,
            "log_activity should return immediately, took {}ms",
            elapsed.as_millis()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_server_activity_returns_paginated_logs(pool: sqlx::PgPool) {
        let (_user_id, token) = seed_admin(&pool).await;
        let server_id = seed_server(&pool, _user_id).await;

        for i in 0..5 {
            sqlx::query(
                "INSERT INTO activity_logs (id, server_id, user_id, event, properties, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
                .bind(Uuid::new_v4().to_string())
                .bind(server_id.to_string())
                .bind(_user_id.to_string())
                .bind(format!("event{}", i))
                .bind("{}")
                .bind(Utc::now().to_rfc3339())
                .execute(&pool)
                .await
                .unwrap();
        }

        let state = make_state(pool).await;
        let app = make_router(state);
        let req = Request::builder()
            .method("GET")
            .uri(&format!("/api/servers/{}/activity?page=1&per_page=2", server_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let logs: Vec<ActivityLog> = serde_json::from_slice(&body).unwrap();
        assert_eq!(logs.len(), 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_all_activity_requires_admin(pool: sqlx::PgPool) {
        let (_user_id, token) = seed_user(&pool).await;
        let state = make_state(pool).await;
        let app = make_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/activity")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_account_activity_filters_by_user(pool: sqlx::PgPool) {
        let (user_id_1, token_1) = seed_user(&pool).await;
        let (user_id_2, _token_2) = seed_user(&pool).await;

        for i in 0..3 {
            sqlx::query(
                "INSERT INTO activity_logs (id, server_id, user_id, event, properties, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
                .bind(Uuid::new_v4().to_string())
                .bind(None::<String>)
                .bind(user_id_1.to_string())
                .bind(format!("event_user1_{}", i))
                .bind("{}")
                .bind(Utc::now().to_rfc3339())
                .execute(&pool)
                .await
                .unwrap();
        }

        for i in 0..2 {
            sqlx::query(
                "INSERT INTO activity_logs (id, server_id, user_id, event, properties, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)"
            )
                .bind(Uuid::new_v4().to_string())
                .bind(None::<String>)
                .bind(user_id_2.to_string())
                .bind(format!("event_user2_{}", i))
                .bind("{}")
                .bind(Utc::now().to_rfc3339())
                .execute(&pool)
                .await
                .unwrap();
        }

        let state = make_state(pool).await;
        let app = make_router(state);
        let req = Request::builder()
            .method("GET")
            .uri("/api/account/activity")
            .header("authorization", format!("Bearer {}", token_1))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let logs: Vec<ActivityLog> = serde_json::from_slice(&body).unwrap();
        assert_eq!(logs.len(), 3);
        assert!(logs.iter().all(|l| l.user_id == Some(user_id_1)));
    }
}
