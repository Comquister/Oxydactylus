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
    permissions::{is_valid_permission, USER_CREATE, USER_DELETE, USER_READ, USER_UPDATE},
    AppState,
};

#[derive(Debug, Serialize)]
pub struct ServerSubuser {
    pub id: Uuid,
    pub server_id: Uuid,
    pub user_id: Uuid,
    pub permissions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for ServerSubuser {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let server_id_str: String = row.try_get("server_id")?;
        let server_id = Uuid::parse_str(&server_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let user_id_str: String = row.try_get("user_id")?;
        let user_id = Uuid::parse_str(&user_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let permissions_str: String = row.try_get("permissions")?;
        let permissions: Vec<String> = serde_json::from_str(&permissions_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            server_id,
            user_id,
            permissions,
            created_at,
        })
    }
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
    db: &sqlx::AnyPool,
) -> Result<()> {
    if user.is_admin {
        return Ok(());
    }
    let owner_id_opt: Option<String> = sqlx::query_scalar("SELECT user_id FROM servers WHERE id = $1")
        .bind(server_id.to_string())
        .fetch_optional(db)
        .await?;
    let owner_id_str = owner_id_opt.ok_or_else(|| PanelError::NotFound(format!("server {}", server_id)))?;
    let owner_id = Uuid::parse_str(&owner_id_str).map_err(|e| PanelError::Internal(e.to_string()))?;
    if owner_id == user.id {
        return Ok(());
    }
    let permissions_str: Option<String> = sqlx::query_scalar(
        "SELECT permissions FROM server_subusers
         WHERE server_id = $1 AND user_id = $2",
    )
    .bind(server_id.to_string())
    .bind(user.id.to_string())
    .fetch_optional(db)
    .await?;

    let perms: Vec<String> = match permissions_str {
        Some(s) => serde_json::from_str(&s).map_err(|e| PanelError::Internal(e.to_string()))?,
        None => return Err(PanelError::Forbidden),
    };

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
    .bind(server_id.to_string())
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
    let id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let sql = crate::db::port_sql(
        "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, server_id, user_id, permissions, created_at",
        &state.db_backend,
    );
    let subuser = sqlx::query_as::<_, ServerSubuser>(&sql)
        .bind(&id)
        .bind(server_id.to_string())
        .bind(body.user_id.to_string())
        .bind(serde_json::to_string(&body.permissions).unwrap())
        .bind(&created_at)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(subuser)))
}

pub async fn update_subuser(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, subuser_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<UpdateSubuserBody>,
) -> Result<Json<ServerSubuser>> {
    check_access(&user, server_id, USER_UPDATE, &state.db).await?;
    for p in &body.permissions {
        if !is_valid_permission(p) {
            return Err(PanelError::Validation(format!("unknown permission: {}", p)));
        }
    }
    let sql = crate::db::port_sql(
        "UPDATE server_subusers SET permissions = $1
         WHERE id = $2 AND server_id = $3
         RETURNING id, server_id, user_id, permissions, created_at",
        &state.db_backend,
    );
    let subuser = sqlx::query_as::<_, ServerSubuser>(&sql)
        .bind(serde_json::to_string(&body.permissions).unwrap())
        .bind(subuser_id.to_string())
        .bind(server_id.to_string())
        .fetch_one(&state.db)
        .await?;
    Ok(Json(subuser))
}

pub async fn delete_subuser(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, subuser_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    check_access(&user, server_id, USER_DELETE, &state.db).await?;
    let sql = crate::db::port_sql(
        "DELETE FROM server_subusers WHERE id = $1 AND server_id = $2",
        &state.db_backend,
    );
    let res = sqlx::query(&sql)
        .bind(subuser_id.to_string())
        .bind(server_id.to_string())
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(PanelError::NotFound(format!("subuser {}", subuser_id)));
    }
    Ok(StatusCode::NO_CONTENT)
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

    async fn seed_admin(pool: &sqlx::PgPool) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1,$2,$3,$4,$5)")
            .bind(id.to_string())
            .bind("a@t.com")
            .bind(&hash)
            .bind(true)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .unwrap();
        let token = encode_token(id, true, "access", SECRET, 900).unwrap();
        (id, token)
    }

    async fn seed_user(pool: &sqlx::PgPool, email: &str) -> (Uuid, String) {
        let id = Uuid::new_v4();
        let hash = hash_password("pass").unwrap();
        sqlx::query("INSERT INTO users (id, email, password_hash, created_at) VALUES ($1,$2,$3,$4)")
            .bind(id.to_string())
            .bind(email)
            .bind(&hash)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .unwrap();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        (id, token)
    }

    async fn seed_node(pool: &sqlx::PgPool) -> Uuid {
        let id_str: String = sqlx::query_scalar(
            "INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1,$2,$3,$4,$5) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("n")
        .bind("http://127.0.0.1:1")
        .bind("tok")
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(pool)
        .await
        .unwrap();
        Uuid::parse_str(&id_str).unwrap()
    }

    async fn seed_server(pool: &sqlx::PgPool, user_id: Uuid, node_id: Uuid, name: &str) -> Uuid {
        let id_str: String = sqlx::query_scalar(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, created_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind(name)
        .bind("ubuntu")
        .bind(512)
        .bind(50)
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(pool)
        .await
        .unwrap();
        Uuid::parse_str(&id_str).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_add_subuser(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "sub@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "sub-srv").await;

        let app = router(make_state(pool).await);
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

        let app = router(make_state(pool).await);
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

        let app = router(make_state(pool).await);
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
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(server_id.to_string())
        .bind(sub_id.to_string())
        .bind(serde_json::to_string(&vec!["control.start"]).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
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
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(server_id.to_string())
        .bind(sub1_id.to_string())
        .bind(serde_json::to_string(&vec!["user.read", "user.create"]).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool.clone()).await);
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

        let app = router(make_state(pool).await);
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

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_update_subuser_permissions(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "upd@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "upd-srv").await;

        let subuser_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(subuser_id.to_string())
        .bind(server_id.to_string())
        .bind(sub_id.to_string())
        .bind(serde_json::to_string(&vec!["control.start"]).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
        let body = serde_json::json!({ "permissions": ["control.start", "control.stop"] });
        let req = Request::builder()
            .method("PATCH")
            .uri(format!("/api/servers/{}/subusers/{}", server_id, subuser_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let su: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let perms = su["permissions"].as_array().unwrap();
        assert!(perms.iter().any(|p| p == "control.stop"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn owner_can_delete_subuser(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let (sub_id, _) = seed_user(&pool, "del@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "del-srv").await;

        let subuser_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(subuser_id.to_string())
        .bind(server_id.to_string())
        .bind(sub_id.to_string())
        .bind(serde_json::to_string(&Vec::<String>::new()).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool.clone()).await);
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/servers/{}/subusers/{}", server_id, subuser_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM server_subusers WHERE id = $1",
        )
        .bind(subuser_id.to_string())
        .fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn non_owner_cannot_delete_subuser(pool: sqlx::PgPool) {
        let (admin_id, _) = seed_admin(&pool).await;
        let (other_id, other_token) = seed_user(&pool, "del2@t.com").await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "del2-srv").await;

        let subuser_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO server_subusers (id, server_id, user_id, permissions, created_at)
             VALUES ($1,$2,$3,$4,$5)",
        )
        .bind(subuser_id.to_string())
        .bind(server_id.to_string())
        .bind(other_id.to_string())
        .bind(serde_json::to_string(&Vec::<String>::new()).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/servers/{}/subusers/{}", server_id, subuser_id))
            .header("authorization", format!("Bearer {}", other_token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_non_existent_subuser_returns_404(pool: sqlx::PgPool) {
        let (admin_id, admin_token) = seed_admin(&pool).await;
        let node_id = seed_node(&pool).await;
        let server_id = seed_server(&pool, admin_id, node_id, "del-nonexist-srv").await;
        let random_subuser_id = Uuid::new_v4();

        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("DELETE")
            .uri(format!("/api/servers/{}/subusers/{}", server_id, random_subuser_id))
            .header("authorization", format!("Bearer {}", admin_token))
            .body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}

