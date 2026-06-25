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
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize, Clone)]
pub struct Node {
    pub id:         Uuid,
    pub name:       String,
    pub grpc_addr:  String,
    #[serde(skip)]
    #[allow(dead_code)]
    pub token:      String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateNodeRequest {
    name:      String,
    grpc_addr: String,
    token:     String,
}

async fn list_nodes(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<Json<Vec<Node>>> {
    let nodes = sqlx::query_as::<_, Node>(
        "SELECT id, name, grpc_addr, token, created_at FROM nodes ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(nodes))
}

async fn create_node(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<Node>)> {
    if body.name.is_empty() || body.grpc_addr.is_empty() || body.token.is_empty() {
        return Err(PanelError::Validation(
            "name, grpc_addr, and token are required".to_string(),
        ));
    }
    let node = sqlx::query_as::<_, Node>(
        "INSERT INTO nodes (name, grpc_addr, token)
         VALUES ($1, $2, $3)
         RETURNING id, name, grpc_addr, token, created_at",
    )
    .bind(&body.name)
    .bind(&body.grpc_addr)
    .bind(&body.token)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(node)))
}

async fn delete_node(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let rows = sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(id.to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn nodes_router() -> Router<AppState> {
    Router::new()
        .route("/",    get(list_nodes).post(create_node))
        .route("/:id", delete(delete_node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::{encode_token, hash_password}, router, AppState};
    use axum::{body::Body, http::{Request, StatusCode}};
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState { db: pool, jwt_secret: SECRET.to_string() }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> String {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1, $2, $3, $4)",
        )
        .bind(id).bind("a@t.com").bind(&hash).bind(true)
        .execute(pool).await.unwrap();
        encode_token(id, true, "access", SECRET, 900).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_node_and_list(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool));

        let body = serde_json::json!({
            "name": "node-eu-1",
            "grpc_addr": "http://10.0.0.1:8080",
            "token": "secret-node-token"
        });
        let create_req = Request::builder()
            .method("POST").uri("/api/nodes")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.clone().oneshot(create_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let list_req = Request::builder()
            .method("GET").uri("/api/nodes")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(list_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 1);
        assert_eq!(json[0]["name"], "node-eu-1");
        assert!(json[0].get("token").is_none(), "token must not be serialized");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_node_returns_204(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        // insert a node directly
        let node_id: Uuid = sqlx::query_scalar(
            "INSERT INTO nodes (name, grpc_addr, token) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("n1").bind("http://localhost:8080").bind("tok")
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("DELETE").uri(format!("/api/nodes/{}", node_id))
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }
}
