use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    async_trait, extract::FromRequestParts, http::request::Parts, response::IntoResponse,
    routing::post, Json, Router,
};
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::{PanelError, Result},
    AppState,
};

// ── Password hashing ──────────────────────────────────────────────────────────

pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| PanelError::Internal(e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).ok().map_or(false, |h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}

// ── JWT ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub adm: bool,
    pub exp: u64,
    pub kind: String,
}

pub fn encode_token(
    user_id: Uuid,
    is_admin: bool,
    kind: &str,
    secret: &str,
    ttl_secs: u64,
) -> Result<String> {
    let now = Utc::now().timestamp() as u64;
    let exp = now.saturating_sub(1).saturating_add(ttl_secs);
    let claims = Claims {
        sub: user_id.to_string(),
        adm: is_admin,
        exp,
        kind: kind.to_string(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| PanelError::Internal(e.to_string()))
}

pub fn decode_token(token: &str, secret: &str, expected_kind: &str) -> Result<Claims> {
    let mut validation = Validation::default();
    validation.leeway = 0;
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|e| PanelError::Unauthorized(e.to_string()))?;
    if data.claims.kind != expected_kind {
        return Err(PanelError::Unauthorized("wrong token type".to_string()));
    }
    Ok(data.claims)
}

// ── Extractors ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub is_admin: bool,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| {
                parts.uri.query().and_then(|q| {
                    q.split('&').find_map(|kv| kv.strip_prefix("token="))
                })
            })
            .ok_or_else(|| {
                PanelError::Unauthorized("missing token".to_string()).into_response()
            })?;
        let claims = decode_token(token, &state.jwt_secret, "access")
            .map_err(IntoResponse::into_response)?;
        let id = Uuid::parse_str(&claims.sub)
            .map_err(|_| PanelError::Unauthorized("invalid sub".to_string()).into_response())?;
        Ok(AuthUser {
            id,
            is_admin: claims.adm,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AdminUser(pub AuthUser);

#[async_trait]
impl FromRequestParts<AppState> for AdminUser {
    type Rejection = axum::response::Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(PanelError::Forbidden.into_response());
        }
        Ok(AdminUser(user))
    }
}

// ── Login / Refresh handlers ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    email: String,
    is_admin: bool,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    password_hash: String,
    is_admin: bool,
}

async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<LoginRequest>,
) -> std::result::Result<Json<TokenResponse>, PanelError> {
    let row: Option<UserRow> = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, password_hash, is_admin FROM users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await?;

    let row = row.ok_or_else(|| PanelError::Unauthorized("invalid credentials".to_string()))?;

    let password = body.password.clone();
    let hash = row.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|e| PanelError::Internal(e.to_string()))?;

    if !valid {
        return Err(PanelError::Unauthorized("invalid credentials".to_string()));
    }

    let access_token = encode_token(row.id, row.is_admin, "access", &state.jwt_secret, 900)?;
    let refresh_token = encode_token(row.id, row.is_admin, "refresh", &state.jwt_secret, 604_800)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
        email: row.email,
        is_admin: row.is_admin,
    }))
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, Serialize)]
struct AccessTokenResponse {
    access_token: String,
}

async fn refresh(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> std::result::Result<Json<AccessTokenResponse>, PanelError> {
    let claims = decode_token(&body.refresh_token, &state.jwt_secret, "refresh")?;
    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| PanelError::Unauthorized("invalid sub".to_string()))?;
    let access_token = encode_token(user_id, claims.adm, "access", &state.jwt_secret, 900)?;
    Ok(Json(AccessTokenResponse { access_token }))
}

pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-secret-at-least-32-chars-long!!";

    #[test]
    fn hash_and_verify_password() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn encode_decode_access_token() {
        let id = Uuid::new_v4();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        let claims = decode_token(&token, SECRET, "access").unwrap();
        assert_eq!(claims.sub, id.to_string());
        assert!(claims.adm);
        assert_eq!(claims.kind, "access");
    }

    #[test]
    fn wrong_kind_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "refresh", SECRET, 900).unwrap();
        assert!(decode_token(&token, SECRET, "access").is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let id = Uuid::new_v4();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        assert!(decode_token(&token, "different-secret", "access").is_err());
    }

    #[test]
    fn expired_token_rejected() {
        let id = Uuid::new_v4();
        // ttl_secs = 0 produces a token that is already expired
        let token = encode_token(id, false, "access", SECRET, 0).unwrap();
        assert!(decode_token(&token, SECRET, "access").is_err());
    }

    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    async fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState {
            db: pool,
            jwt_secret: SECRET.to_string(),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_with_valid_credentials_returns_tokens(pool: sqlx::PgPool) {
        let state = make_state(pool.clone()).await;
        let hash = hash_password("password123").unwrap();
        sqlx::query("INSERT INTO users (email, password_hash, is_admin) VALUES ($1, $2, $3)")
            .bind("admin@example.com")
            .bind(&hash)
            .bind(true)
            .execute(&pool)
            .await
            .unwrap();

        let app = crate::router(state);
        let body = serde_json::json!({ "email": "admin@example.com", "password": "password123" });
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);

        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(json["access_token"].is_string());
        assert!(json["refresh_token"].is_string());
        assert_eq!(json["email"].as_str(), Some("admin@example.com"));
        assert_eq!(json["is_admin"].as_bool(), Some(true));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_with_wrong_password_returns_401(pool: sqlx::PgPool) {
        let state = make_state(pool.clone()).await;
        let hash = hash_password("correct").unwrap();
        sqlx::query("INSERT INTO users (email, password_hash, is_admin) VALUES ($1, $2, $3)")
            .bind("user@example.com")
            .bind(&hash)
            .bind(false)
            .execute(&pool)
            .await
            .unwrap();

        let app = crate::router(state);
        let body = serde_json::json!({ "email": "user@example.com", "password": "wrong" });
        let req = Request::builder()
            .method("POST")
            .uri("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_via_query_token_param(pool: sqlx::PgPool) {
        let state = make_state(pool.clone()).await;
        let hash = hash_password("pass").unwrap();
        let user_id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, password_hash, is_admin) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind("q@example.com")
        .bind(&hash)
        .bind(false)
        .fetch_one(&pool)
        .await
        .unwrap();

        let token = encode_token(user_id, false, "access", SECRET, 900).unwrap();

        // GET /api/me com token na query string
        let app = crate::router(state);
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/me?token={}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), axum::http::StatusCode::OK);
    }
}
