use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct Egg {
    pub id:            Uuid,
    pub name:          String,
    pub description:   Option<String>,
    pub author:        Option<String>,
    pub version:       String,
    pub features:      Vec<String>,
    pub file_denylist: Vec<String>,
    pub docker_images: serde_json::Value,
    pub start_cmd:     String,
    pub stop_cmd:      String,
    pub startup_done:  Option<String>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateEggRequest {
    name:          String,
    #[serde(default)]
    description:   Option<String>,
    #[serde(default)]
    author:        Option<String>,
    #[serde(default = "default_version")]
    version:       String,
    #[serde(default)]
    features:      Vec<String>,
    #[serde(default)]
    file_denylist: Vec<String>,
    #[serde(default = "empty_object")]
    docker_images: serde_json::Value,
    start_cmd:     String,
    #[serde(default = "default_stop")]
    stop_cmd:      String,
    #[serde(default)]
    startup_done:  Option<String>,
}

fn default_version() -> String { "1.0.0".to_string() }
fn default_stop()    -> String { "stop".to_string() }
fn empty_object()    -> serde_json::Value { serde_json::json!({}) }

async fn list_eggs(
    State(state): State<AppState>,
    _user: AuthUser,
) -> Result<Json<Vec<Egg>>> {
    let eggs = sqlx::query_as::<_, Egg>(
        "SELECT id, name, description, author, version, features, file_denylist,
                docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at
         FROM eggs ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(eggs))
}

async fn create_egg(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateEggRequest>,
) -> Result<(StatusCode, Json<Egg>)> {
    if body.name.is_empty() || body.start_cmd.is_empty() {
        return Err(PanelError::Validation("name and start_cmd are required".into()));
    }
    let egg = sqlx::query_as::<_, Egg>(
        "INSERT INTO eggs
             (name, description, author, version, features, file_denylist,
              docker_images, start_cmd, stop_cmd, startup_done)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING id, name, description, author, version, features, file_denylist,
                   docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at",
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.author)
    .bind(&body.version)
    .bind(&body.features)
    .bind(&body.file_denylist)
    .bind(&body.docker_images)
    .bind(&body.start_cmd)
    .bind(&body.stop_cmd)
    .bind(&body.startup_done)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(egg)))
}

async fn get_egg(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Egg>> {
    let egg = sqlx::query_as::<_, Egg>(
        "SELECT id, name, description, author, version, features, file_denylist,
                docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at
         FROM eggs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(egg))
}

async fn delete_egg(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let rows = sqlx::query("DELETE FROM eggs WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(format!("egg {}", id)));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn eggs_router() -> Router<AppState> {
    Router::new()
        .route("/",    get(list_eggs).post(create_egg))
        .route("/:id", get(get_egg).delete(delete_egg))
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
    async fn list_eggs_empty(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET").uri("/api/eggs")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_and_get_egg(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool));
        let body = serde_json::json!({
            "name": "Purpur",
            "start_cmd": "java -jar server.jar",
            "stop_cmd": "stop",
            "docker_images": {"Java 21": "ghcr.io/ptero-eggs/yolks:java_21"}
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("POST").uri("/api/eggs")
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let egg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let egg_id = egg["id"].as_str().unwrap();

        let res2 = app.oneshot(
            Request::builder()
                .method("GET").uri(format!("/api/eggs/{}", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes2 = res2.into_body().collect().await.unwrap().to_bytes();
        let got: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        assert_eq!(got["name"], "Purpur");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_egg(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id: Uuid = sqlx::query_scalar(
            "INSERT INTO eggs (name, start_cmd, docker_images) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("test-egg").bind("./run.sh").bind(serde_json::json!({}))
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let res = app.oneshot(
            Request::builder()
                .method("DELETE").uri(format!("/api/eggs/{}", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }
}
