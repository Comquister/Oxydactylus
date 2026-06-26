use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{hash_password, AdminUser, AuthUser},
    error::{PanelError, Result},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for User {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Self {
            id,
            email: row.try_get("email")?,
            is_admin: row.try_get("is_admin")?,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateUserRequest {
    email: String,
    password: String,
    is_admin: bool,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub email: String,
    pub is_admin: bool,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for MeResponse {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Self {
            id,
            email: row.try_get("email")?,
            is_admin: row.try_get("is_admin")?,
        })
    }
}

async fn list_users(State(state): State<AppState>, _admin: AdminUser) -> Result<Json<Vec<User>>> {
    let sql = crate::db::port_sql(
        "SELECT id, email, password_hash, is_admin, created_at FROM users ORDER BY created_at",
        &state.db_backend,
    );
    let users = sqlx::query_as::<_, User>(&sql)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(users))
}

async fn get_user(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<User>> {
    if !caller.is_admin && caller.id != id {
        return Err(PanelError::Forbidden);
    }
    let sql = crate::db::port_sql(
        "SELECT id, email, password_hash, is_admin, created_at FROM users WHERE id = $1",
        &state.db_backend,
    );
    let user = sqlx::query_as::<_, User>(&sql)
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;
    Ok(Json(user))
}

async fn create_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<User>)> {
    if body.email.is_empty() || body.password.len() < 8 {
        return Err(PanelError::Validation(
            "email required; password must be at least 8 characters".to_string(),
        ));
    }
    let password = body.password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))??;

    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let sql = crate::db::port_sql(
        "INSERT INTO users (id, email, password_hash, is_admin, created_at)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, email, password_hash, is_admin, created_at",
        &state.db_backend,
    );
    let user = sqlx::query_as::<_, User>(&sql)
        .bind(&id)
        .bind(&body.email)
        .bind(&hash)
        .bind(body.is_admin)
        .bind(&created_at)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn delete_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let sql = crate::db::port_sql(
        "DELETE FROM users WHERE id = $1",
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
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<MeResponse>> {
    let row =
        sqlx::query_as::<_, MeResponse>("SELECT id, email, is_admin FROM users WHERE id = $1")
            .bind(user.id.to_string())
            .fetch_one(&state.db)
            .await?;
    Ok(Json(row))
}

pub fn users_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_users).post(create_user))
        .route("/:id", get(get_user).delete(delete_user))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::hash_password, router, AppState};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    async fn make_state(pool: sqlx::PgPool) -> AppState {
        use sqlx::ConnectOptions;
        sqlx::any::install_default_drivers();
        let db_url = pool.connect_options().to_url_lossy().to_string();
        let any_pool = sqlx::AnyPool::connect(&db_url).await.unwrap();
        AppState {
            db: any_pool,
            db_backend: "PostgreSQL".to_string(),
            jwt_secret: "test-secret-at-least-32-chars-long!!".to_string(),
            app_key: None,
        }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("admin-pass").unwrap();
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
        // return JWT for admin
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
    async fn list_users_requires_auth(pool: sqlx::PgPool) {
        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/users")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_users_requires_admin(pool: sqlx::PgPool) {
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("user@test.com")
        .bind(hash_password("pass").unwrap())
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
            .uri("/api/users")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_can_list_users(pool: sqlx::PgPool) {
        let (_id, token) = seed_admin(&pool).await;
        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/users")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json.as_array().unwrap().len() >= 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_can_create_user(pool: sqlx::PgPool) {
        let (_id, token) = seed_admin(&pool).await;
        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "email": "new@test.com",
            "password": "newpassword",
            "is_admin": false
        });
        let req = Request::builder()
            .method("POST")
            .uri("/api/users")
            .header("authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn me_returns_current_user(pool: sqlx::PgPool) {
        let id = Uuid::new_v4();
        let hash = hash_password("admin-pass").unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind("a@t.com")
        .bind(&hash)
        .bind(true)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
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

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/me")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let me: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(me["email"], "a@t.com");
        assert_eq!(me["is_admin"], true);
        assert!(me["id"].as_str().is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn me_requires_auth(pool: sqlx::PgPool) {
        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/me")
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
