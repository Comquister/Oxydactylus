use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::{PanelError, Result},
    permissions::{is_valid_permission, USER_CREATE, USER_READ},
    AppState,
};

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ServerSubuser {
    pub id: Uuid,
    pub server_id: Uuid,
    pub user_id: Uuid,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SubuserBody {
    pub user_id: Uuid,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSubuserBody {
    pub permissions: Vec<String>,
}

/// Verifica se o usuário é admin, dono do servidor ou subuser com a permissão dada.
async fn check_access(
    user: &AuthUser,
    server_id: Uuid,
    perm: &str,
    db: &sqlx::PgPool,
) -> Result<()> {
    if user.is_admin {
        return Ok(());
    }
    let owner_id: Option<Uuid> = sqlx::query_scalar("SELECT user_id FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(db)
        .await?;
    let owner_id = owner_id.ok_or_else(|| PanelError::NotFound(format!("server {}", server_id)))?;
    if owner_id == user.id {
        return Ok(());
    }
    let perms: Vec<String> = sqlx::query_scalar(
        "SELECT unnest(permissions) FROM server_subusers
         WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id)
    .bind(user.id)
    .fetch_all(db)
    .await?;
    if perms.iter().any(|p| p == perm) {
        Ok(())
    } else {
        Err(PanelError::Forbidden)
    }
}

pub async fn list_subusers(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<ServerSubuser>>> {
    check_access(&user, server_id, USER_READ, &state.db).await?;
    let subusers = sqlx::query_as::<_, ServerSubuser>(
        "SELECT id, server_id, user_id, permissions, created_at
         FROM server_subusers WHERE server_id = $1 ORDER BY created_at",
    )
    .bind(server_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(subusers))
}

pub async fn create_subuser(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<SubuserBody>,
) -> Result<(StatusCode, Json<ServerSubuser>)> {
    check_access(&user, server_id, USER_CREATE, &state.db).await?;
    for p in &body.permissions {
        if !is_valid_permission(p) {
            return Err(PanelError::Validation(format!("unknown permission: {}", p)));
        }
    }
    let subuser = sqlx::query_as::<_, ServerSubuser>(
        "INSERT INTO server_subusers (server_id, user_id, permissions)
         VALUES ($1, $2, $3)
         RETURNING id, server_id, user_id, permissions, created_at",
    )
    .bind(server_id)
    .bind(body.user_id)
    .bind(&body.permissions)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(subuser)))
}

#[cfg(test)]
mod tests {
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

    fn make_state(pool: sqlx::PgPool) -> AppState {
        AppState {
            db: pool,
            jwt_secret: SECRET.to_string(),
        }
    }

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, email, password_hash, is_admin) VALUES ($1,$2,$3,$4)")
            .bind(id)
            .bind("a@t.com")
            .bind(&hash)
            .bind(true)
            .execute(pool)
            .await
            .unwrap();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        (id, token)
    }

    async fn seed_user(pool: &sqlx::PgPool, email: &str) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, email, password_hash) VALUES ($1,$2,$3)")
            .bind(id)
            .bind(email)
            .bind(&hash)
            .execute(pool)
            .await
            .unwrap();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        (id, token)
    }

    async fn seed_node(pool: &sqlx::PgPool) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO nodes (name, grpc_addr, token) VALUES ($1,$2,$3) RETURNING id",
        )
        .bind("n")
        .bind("http://127.0.0.1:1")
        .bind("tok")
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn seed_server(pool: &sqlx::PgPool, user_id: Uuid, node_id: Uuid, name: &str) -> Uuid {
        sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO servers (user_id, node_id, name, image, memory_mb, cpu_percent)
             VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
        )
        .bind(user_id)
        .bind(node_id)
        .bind(name)
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_add_subuser(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "sub@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "sub-srv").await;

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "user_id":     sub_id,
            "permissions": ["control.start", "control.stop"],
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let su: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(su["user_id"].as_str().unwrap(), sub_id.to_string());
        let perms = su["permissions"].as_array().unwrap();
        assert!(perms.iter().any(|p| p == "control.start"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn non_owner_cannot_add_subuser(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let (other_id, other_token) = seed_user(&pool, "other@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "perm-srv").await;

        let app = router(make_state(pool));
        let body = serde_json::json!({ "user_id": other_id, "permissions": [] });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", other_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_permission_rejected(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "inv@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "inv-srv").await;

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "user_id":     sub_id,
            "permissions": ["hacker.pwn"],
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_list_subusers(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "list@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "list-srv").await;

        sqlx::query(
            "INSERT INTO server_subusers (server_id, user_id, permissions)
             VALUES ($1,$2,ARRAY['control.start'])",
        )
        .bind(server_id)
        .bind(sub_id)
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool));
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn subuser_with_permission_can_list_and_add(pool: sqlx::PgPool) {
        let (owner_id, _) = seed_user(&pool, "owner@t.com").await;
        let (sub1_id, sub1_token) = seed_user(&pool, "sub1@t.com").await;
        let (sub2_id, _) = seed_user(&pool, "sub2@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, owner_id, node_id, "perm-srv").await;

        sqlx::query(
            "INSERT INTO server_subusers (server_id, user_id, permissions)
             VALUES ($1, $2, ARRAY['user.read', 'user.create'])",
        )
        .bind(server_id)
        .bind(sub1_id)
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool.clone()));
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", sub1_token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "user_id":     sub2_id,
            "permissions": ["control.start"],
        });
        let req = Request::builder()
            .method("POST")
            .uri(format!("/api/servers/{}/subusers", server_id))
            .header("authorization", format!("Bearer {}", sub1_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }
}
