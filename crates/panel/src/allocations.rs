use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::{
    auth::AdminUser,
    db::port_sql,
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Allocation {
    pub id: Uuid,
    pub node_id: Uuid,
    pub ip: String,
    pub ip_alias: Option<String>,
    pub port: i32,
    pub server_id: Option<Uuid>,
    pub created_at: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Allocation {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let node_id_str: String = row.try_get("node_id")?;
        let node_id = Uuid::parse_str(&node_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let server_id_opt: Option<String> = row.try_get("server_id")?;
        let server_id = match server_id_opt {
            Some(s) if !s.is_empty() => Some(Uuid::parse_str(&s).map_err(|e| sqlx::Error::Decode(Box::new(e)))?),
            _ => None,
        };

        Ok(Self {
            id,
            node_id,
            ip: row.try_get("ip")?,
            ip_alias: row.try_get("ip_alias")?,
            port: row.try_get("port")?,
            server_id,
            created_at: row.try_get("created_at")?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateAllocationsRequest {
    pub ip: String,
    pub ip_alias: Option<String>,
    pub ports: Vec<serde_json::Value>, // Supports both e.g. "3000-3010" and 3000
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/:id/allocations", get(list_allocations).post(create_allocations))
        .route("/:id/allocations/:aid", axum::routing::delete(delete_allocation))
}

async fn list_allocations(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(node_id): Path<Uuid>,
) -> Result<Json<Vec<Allocation>>> {
    let sql = port_sql("SELECT * FROM allocations WHERE node_id = $1 ORDER BY port ASC", &state.db_backend);
    let list = sqlx::query_as::<_, Allocation>(&sql)
        .bind(node_id.to_string())
        .fetch_all(&state.db)
        .await?;
    Ok(Json(list))
}

async fn create_allocations(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(node_id): Path<Uuid>,
    Json(body): Json<CreateAllocationsRequest>,
) -> Result<StatusCode> {
    if body.ip.trim().is_empty() {
        return Err(PanelError::Validation("IP address cannot be empty".to_string()));
    }

    let mut resolved_ports = Vec::new();
    for port_val in body.ports {
        match port_val {
            serde_json::Value::Number(n) => {
                let p = n.as_i64().ok_or_else(|| PanelError::Validation("invalid port format".to_string()))?;
                if p <= 0 || p > 65535 {
                    return Err(PanelError::Validation("invalid port bounds".to_string()));
                }
                resolved_ports.push(p as i32);
            }
            serde_json::Value::String(port_str) => {
                let port_str = port_str.trim();
                if port_str.contains('-') {
                    let parts: Vec<&str> = port_str.split('-').collect();
                    if parts.len() == 2 {
                        let start: i32 = parts[0].trim().parse().map_err(|_| PanelError::Validation("invalid port range".to_string()))?;
                        let end: i32 = parts[1].trim().parse().map_err(|_| PanelError::Validation("invalid port range".to_string()))?;
                        if start > end || start <= 0 || end > 65535 {
                            return Err(PanelError::Validation("invalid port range bounds".to_string()));
                        }
                        for p in start..=end {
                            resolved_ports.push(p);
                        }
                    } else {
                        return Err(PanelError::Validation("invalid port range format".to_string()));
                    }
                } else {
                    let p: i32 = port_str.parse().map_err(|_| PanelError::Validation("invalid port".to_string()))?;
                    if p <= 0 || p > 65535 {
                        return Err(PanelError::Validation("invalid port bounds".to_string()));
                    }
                    resolved_ports.push(p);
                }
            }
            _ => {
                return Err(PanelError::Validation("invalid port format".to_string()));
            }
        }
    }

    if resolved_ports.is_empty() {
        return Err(PanelError::Validation("ports list cannot be empty".to_string()));
    }

    let created_at = chrono::Utc::now().to_rfc3339();
    for port in resolved_ports {
        let id = Uuid::new_v4().to_string();
        let sql = port_sql(
            "INSERT INTO allocations (id, node_id, ip, ip_alias, port, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT(node_id, ip, port) DO NOTHING",
            &state.db_backend
        );
        sqlx::query(&sql)
            .bind(&id)
            .bind(node_id.to_string())
            .bind(&body.ip)
            .bind(&body.ip_alias)
            .bind(port)
            .bind(&created_at)
            .execute(&state.db)
            .await?;
    }

    Ok(StatusCode::CREATED)
}

async fn delete_allocation(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((_node_id, allocation_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let check_sql = port_sql("SELECT server_id FROM allocations WHERE id = $1", &state.db_backend);
    let server_id: Option<Option<String>> = sqlx::query_scalar(&check_sql)
        .bind(allocation_id.to_string())
        .fetch_optional(&state.db)
        .await?;

    match server_id {
        None => return Err(PanelError::NotFound("allocation not found".to_string())),
        Some(Some(s)) if !s.trim().is_empty() => {
            return Err(PanelError::Validation("cannot delete allocation assigned to a server".to_string()));
        }
        _ => {}
    }

    let delete_sql = port_sql("DELETE FROM allocations WHERE id = $1", &state.db_backend);
    sqlx::query(&delete_sql)
        .bind(allocation_id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::{encode_token, hash_password},
        router, AppState,
    };
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    async fn make_state(pool: sqlx::PgPool) -> AppState {
        use sqlx::ConnectOptions;
        sqlx::any::install_default_drivers();
        let db_url = pool.connect_options().to_url_lossy().to_string();
        let any_pool = sqlx::AnyPool::connect(&db_url).await.unwrap();
        AppState {
            db: any_pool,
            db_backend: "PostgreSQL".to_string(),
            jwt_secret: SECRET.to_string(),
            app_key: None,
        }
    }

    async fn seed_admin_and_get_user_id(pool: &sqlx::PgPool) -> (String, Uuid) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind(format!("admin-{}@t.com", Uuid::new_v4()))
        .bind(&hash)
        .bind(true)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
        (encode_token(id, true, "access", SECRET, 900).unwrap(), id)
    }

    async fn seed_node(pool: &sqlx::PgPool) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind(format!("node-{}", Uuid::new_v4()))
        .bind("http://localhost:8080")
        .bind("node-token")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
        id
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_create_and_list_allocations(pool: sqlx::PgPool) {
        let (token, _admin_id) = seed_admin_and_get_user_id(&pool).await;
        let node_id = seed_node(&pool).await;
        let app = router(make_state(pool).await);

        // 1. Create allocations with mix of integer, single-port string, and port range string
        let body = serde_json::json!({
            "ip": "127.0.0.1",
            "ip_alias": "localhost",
            "ports": [3000, "3001", "3005-3007"]
        });

        let create_req = Request::builder()
            .method("POST")
            .uri(format!("/api/nodes/{}/allocations", node_id))
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let res = app.clone().oneshot(create_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        // 2. List allocations
        let list_req = Request::builder()
            .method("GET")
            .uri(format!("/api/nodes/{}/allocations", node_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(list_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let list: Vec<Allocation> = serde_json::from_slice(&bytes).unwrap();
        
        // Output should be sorted by port ASC: 3000, 3001, 3005, 3006, 3007
        assert_eq!(list.len(), 5);
        assert_eq!(list[0].port, 3000);
        assert_eq!(list[1].port, 3001);
        assert_eq!(list[2].port, 3005);
        assert_eq!(list[3].port, 3006);
        assert_eq!(list[4].port, 3007);
        assert_eq!(list[0].ip, "127.0.0.1");
        assert_eq!(list[0].ip_alias, Some("localhost".to_string()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_create_allocations_validation(pool: sqlx::PgPool) {
        let (token, _admin_id) = seed_admin_and_get_user_id(&pool).await;
        let node_id = seed_node(&pool).await;
        let app = router(make_state(pool).await);

        // Case A: empty IP
        let body = serde_json::json!({
            "ip": "   ",
            "ports": [3000]
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/nodes/{}/allocations", node_id))
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Case B: invalid port range bounds
        let body = serde_json::json!({
            "ip": "10.0.0.1",
            "ports": ["3010-3000"]
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/nodes/{}/allocations", node_id))
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // Case C: invalid port format
        let body = serde_json::json!({
            "ip": "10.0.0.1",
            "ports": ["abc"]
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/nodes/{}/allocations", node_id))
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_delete_allocation_flow(pool: sqlx::PgPool) {
        let (token, admin_id) = seed_admin_and_get_user_id(&pool).await;
        let node_id = seed_node(&pool).await;
        
        // Seed an allocation
        let allocation_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO allocations (id, node_id, ip, port, created_at) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(allocation_id.to_string())
        .bind(node_id.to_string())
        .bind("127.0.0.1")
        .bind(8000)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool.clone()).await);

        // 1. Trying to delete a non-existent allocation should return 404
        let fake_id = Uuid::new_v4();
        let res = app.clone().oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/nodes/{}/allocations/{}", node_id, fake_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // 2. Assign the allocation to a server
        let _server_id = Uuid::new_v4();
        sqlx::query("UPDATE allocations SET server_id = $1 WHERE id = $2")
            .bind(_server_id.to_string())
            .bind(allocation_id.to_string())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, allocation_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(_server_id.to_string())
        .bind(admin_id.to_string())
        .bind(node_id.to_string())
        .bind("test-srv")
        .bind("ubuntu:latest")
        .bind(1024)
        .bind(100)
        .bind(allocation_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        // 3. Trying to delete an assigned allocation should return Validation error (422)
        let res = app.clone().oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/nodes/{}/allocations/{}", node_id, allocation_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);

        // 4. Unassign/delete server, then delete allocation successfully
        sqlx::query("DELETE FROM servers WHERE id = $1")
            .bind(_server_id.to_string())
            .execute(&pool)
            .await
            .unwrap();
        
        sqlx::query("UPDATE allocations SET server_id = NULL WHERE id = $1")
            .bind(allocation_id.to_string())
            .execute(&pool)
            .await
            .unwrap();

        let res = app.clone().oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/nodes/{}/allocations/{}", node_id, allocation_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        // Verify it is gone
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM allocations WHERE id = $1")
            .bind(allocation_id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
