use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{
    auth::{AdminUser, AuthUser},
    error::{PanelError, Result},
    servers::load_server_with_access,
    AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct StartupVariable {
    pub env_variable: String,
    pub name: String,
    pub description: Option<String>,
    pub value: String,
    pub default_val: Option<String>,
    pub user_editable: bool,
    pub user_viewable: bool,
    pub field_type: String,
    pub rules: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateStartupRequest {
    variables: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct UpdateDockerImageRequest {
    image: String,
}

async fn get_startup(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<StartupVariable>>> {
    let server = load_server_with_access(&state, &user, server_id, None).await?;

    let Some(egg_id) = server.egg_id else {
        return Ok(Json(Vec::new()));
    };

    let sql = crate::db::port_sql(
        "SELECT id, env_variable, name, description, default_val, user_editable, user_viewable, field_type, rules
         FROM egg_variables WHERE egg_id = $1",
        &state.db_backend,
    );
    let egg_vars: Vec<(String, String, String, Option<String>, Option<String>, bool, bool, String, Option<String>)> =
        sqlx::query_as(&sql)
            .bind(egg_id.to_string())
            .fetch_all(&state.db)
            .await?;

    let current_env: HashMap<String, String> = server
        .env
        .iter()
        .filter_map(|s| {
            let (k, v) = s.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();

    let mut result = Vec::new();
    for (_id, env_var, name, desc, default, user_editable, user_viewable, field_type, rules) in egg_vars {
        if !user.is_admin && !user_viewable {
            continue;
        }

        let value = current_env
            .get(&env_var)
            .cloned()
            .or(default.clone())
            .unwrap_or_default();

        result.push(StartupVariable {
            env_variable: env_var,
            name,
            description: desc,
            value,
            default_val: default,
            user_editable,
            user_viewable,
            field_type,
            rules,
        });
    }

    Ok(Json(result))
}

async fn update_startup(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<UpdateStartupRequest>,
) -> Result<Json<Vec<StartupVariable>>> {
    let server = load_server_with_access(&state, &user, server_id, None).await?;

    let Some(egg_id) = server.egg_id else {
        return Err(PanelError::Validation(
            "Server has no egg assigned".to_string(),
        ));
    };

    let sql = crate::db::port_sql(
        "SELECT id, env_variable, name, description, default_val, user_editable, user_viewable, field_type, rules
         FROM egg_variables WHERE egg_id = $1",
        &state.db_backend,
    );
    let egg_vars: Vec<(String, String, String, Option<String>, Option<String>, bool, bool, String, Option<String>)> =
        sqlx::query_as(&sql)
            .bind(egg_id.to_string())
            .fetch_all(&state.db)
            .await?;

    let mut current_env: HashMap<String, String> = server
        .env
        .iter()
        .filter_map(|s| {
            let (k, v) = s.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect();

    for (_, env_var, _, _, _, user_editable, _, _, rules) in &egg_vars {
        if let Some(new_val) = body.variables.get(env_var) {
            if !user_editable && !user.is_admin {
                return Err(PanelError::Validation(format!(
                    "{} is not user-editable",
                    env_var
                )));
            }

            if let Some(r) = rules {
                crate::egg_vars::validate_var(env_var, new_val, r)?;
            }

            current_env.insert(env_var.clone(), new_val.clone());
        }
    }

    let new_env: Vec<String> = current_env
        .into_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();

    let sql = crate::db::port_sql(
        "UPDATE servers SET env = $1 WHERE id = $2",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(serde_json::to_string(&new_env).unwrap())
        .bind(server_id.to_string())
        .execute(&state.db)
        .await?;

    let mut result = Vec::new();
    for (_id, env_var, name, desc, default, user_editable, user_viewable, field_type, rules) in egg_vars {
        if !user.is_admin && !user_viewable {
            continue;
        }

        let value = new_env
            .iter()
            .find_map(|s| {
                let (k, v) = s.split_once('=')?;
                if k == env_var { Some(v.to_string()) } else { None }
            })
            .or(default.clone())
            .unwrap_or_default();

        result.push(StartupVariable {
            env_variable: env_var,
            name,
            description: desc,
            value,
            default_val: default,
            user_editable,
            user_viewable,
            field_type,
            rules,
        });
    }

    Ok(Json(result))
}

async fn update_docker_image(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<UpdateDockerImageRequest>,
) -> Result<StatusCode> {
    if body.image.is_empty() {
        return Err(PanelError::Validation("image cannot be empty".to_string()));
    }

    let sql = crate::db::port_sql(
        "UPDATE servers SET image = $1 WHERE id = $2",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(&body.image)
        .bind(server_id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub fn startup_router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_startup).put(update_startup))
        .route("/docker-image", put(update_docker_image))
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
    use oxy_core::proto::node::{
        node_service_server::NodeService,
        CreateBackupReply, CreateBackupRequest, DeleteBackupRequest,
        ServerProvisionRequest, ServerReply, ServerStartRequest, ServerStopRequest,
        ServerDeleteRequest, ServerCommandRequest, ServerStats, ServerStatsRequest,
        ServerLogsRequest, LogLine, ListFilesRequest, ListFilesReply,
        GetFileContentsRequest, GetFileContentsReply, WriteFileContentsRequest,
        CreateDirectoryRequest, DeleteFilesRequest, RenameFileRequest,
        DownloadFileRequest, FileChunk,
    };
    use tokio_stream::wrappers::ReceiverStream;
    use tonic::{async_trait, Request as GrpcRequest, Response, Status};
    use tower::ServiceExt;

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
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
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
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_admin, created_at) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id.to_string())
        .bind(email)
        .bind(&hash)
        .bind(false)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(pool)
        .await
        .unwrap();
        let token = encode_token(id, false, "access", SECRET, 900).unwrap();
        (id, token)
    }

    struct AcceptAllNode;

    #[async_trait]
    impl NodeService for AcceptAllNode {
        type StreamLogsStream = ReceiverStream<std::result::Result<LogLine, Status>>;
        type DownloadFileStream = ReceiverStream<std::result::Result<FileChunk, Status>>;

        async fn provision_server(
            &self,
            _: GrpcRequest<ServerProvisionRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn start_server(
            &self,
            _: GrpcRequest<ServerStartRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn stop_server(
            &self,
            _: GrpcRequest<ServerStopRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn delete_server(
            &self,
            _: GrpcRequest<ServerDeleteRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn send_command(
            &self,
            _: GrpcRequest<ServerCommandRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn get_stats(
            &self,
            _: GrpcRequest<ServerStatsRequest>,
        ) -> std::result::Result<Response<ServerStats>, Status> {
            Ok(Response::new(ServerStats::default()))
        }
        async fn stream_logs(
            &self,
            _: GrpcRequest<ServerLogsRequest>,
        ) -> std::result::Result<Response<Self::StreamLogsStream>, Status> {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(ReceiverStream::new(rx)))
        }
        async fn list_files(
            &self,
            _: GrpcRequest<ListFilesRequest>,
        ) -> std::result::Result<Response<ListFilesReply>, Status> {
            Ok(Response::new(ListFilesReply { files: vec![] }))
        }
        async fn get_file_contents(
            &self,
            _: GrpcRequest<GetFileContentsRequest>,
        ) -> std::result::Result<Response<GetFileContentsReply>, Status> {
            Ok(Response::new(GetFileContentsReply { content: vec![] }))
        }
        async fn write_file_contents(
            &self,
            _: GrpcRequest<WriteFileContentsRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn create_directory(
            &self,
            _: GrpcRequest<CreateDirectoryRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn delete_files(
            &self,
            _: GrpcRequest<DeleteFilesRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn rename_file(
            &self,
            _: GrpcRequest<RenameFileRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn download_file(
            &self,
            _: GrpcRequest<DownloadFileRequest>,
        ) -> std::result::Result<Response<Self::DownloadFileStream>, Status> {
            let (_, rx) = tokio::sync::mpsc::channel(1);
            Ok(Response::new(ReceiverStream::new(rx)))
        }
        async fn upload_file(
            &self,
            _: GrpcRequest<tonic::Streaming<FileChunk>>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
        async fn create_backup(
            &self,
            _: GrpcRequest<CreateBackupRequest>,
        ) -> std::result::Result<Response<CreateBackupReply>, Status> {
            Ok(Response::new(CreateBackupReply {
                success: true,
                message: "ok".into(),
                sha256: "abc123".into(),
                bytes: 1000,
            }))
        }
        async fn delete_backup(
            &self,
            _: GrpcRequest<DeleteBackupRequest>,
        ) -> std::result::Result<Response<ServerReply>, Status> {
            Ok(Response::new(ServerReply {
                success: true,
                message: "ok".into(),
            }))
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_get_startup_returns_user_viewable_vars(pool: sqlx::PgPool) {
        let (user_id, token) = seed_user(&pool, "u@t.com").await;
        let state = make_state(pool).await;

        let node_id = Uuid::new_v4();
        sqlx::query("INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(node_id.to_string())
            .bind("test-node")
            .bind("127.0.0.1:50051")
            .bind("test-token")
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        let egg_id = Uuid::new_v4();
        sqlx::query("INSERT INTO eggs (id, name, start_cmd, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(egg_id.to_string())
            .bind("test-egg")
            .bind("java -jar server.jar")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO egg_variables (id, egg_id, env_variable, name, user_viewable, user_editable, default_val)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(Uuid::new_v4().to_string())
        .bind(egg_id.to_string())
        .bind("PUBLIC_VAR")
        .bind("Public Variable")
        .bind(true)
        .bind(true)
        .bind("public_default")
        .execute(&state.db)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO egg_variables (id, egg_id, env_variable, name, user_viewable, user_editable, default_val)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(Uuid::new_v4().to_string())
        .bind(egg_id.to_string())
        .bind("HIDDEN_VAR")
        .bind("Hidden Variable")
        .bind(false)
        .bind(true)
        .bind("hidden_default")
        .execute(&state.db)
        .await
        .unwrap();

        let server_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, env, egg_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(server_id.to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind("test-server")
        .bind("ubuntu:latest")
        .bind(2048)
        .bind(50)
        .bind(serde_json::to_string(&vec!["PUBLIC_VAR=custom_value"]).unwrap())
        .bind(egg_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .unwrap();

        let app = router(state.clone());
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/servers/{}/startup", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let vars: Vec<StartupVariable> = serde_json::from_slice(&body).unwrap();

        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].env_variable, "PUBLIC_VAR");
        assert_eq!(vars[0].value, "custom_value");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_put_startup_rejects_non_editable_vars(pool: sqlx::PgPool) {
        let (user_id, token) = seed_user(&pool, "u@t.com").await;
        let state = make_state(pool).await;

        let node_id = Uuid::new_v4();
        sqlx::query("INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(node_id.to_string())
            .bind("test-node")
            .bind("127.0.0.1:50051")
            .bind("test-token")
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        let egg_id = Uuid::new_v4();
        sqlx::query("INSERT INTO eggs (id, name, start_cmd, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(egg_id.to_string())
            .bind("test-egg")
            .bind("java -jar server.jar")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO egg_variables (id, egg_id, env_variable, name, user_viewable, user_editable, default_val)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(Uuid::new_v4().to_string())
        .bind(egg_id.to_string())
        .bind("READ_ONLY_VAR")
        .bind("Read Only Variable")
        .bind(true)
        .bind(false)
        .bind("readonly_default")
        .execute(&state.db)
        .await
        .unwrap();

        let server_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, env, egg_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(server_id.to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind("test-server")
        .bind("ubuntu:latest")
        .bind(2048)
        .bind(50)
        .bind(serde_json::to_string(&vec!["READ_ONLY_VAR=readonly_default"]).unwrap())
        .bind(egg_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .unwrap();

        let app = router(state.clone());
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/api/servers/{}/startup", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(
                Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "variables": {
                            "READ_ONLY_VAR": "new_value"
                        }
                    }))
                    .unwrap(),
                ),
            )
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_put_startup_updates_env(pool: sqlx::PgPool) {
        let (user_id, token) = seed_user(&pool, "u@t.com").await;
        let state = make_state(pool).await;

        let node_id = Uuid::new_v4();
        sqlx::query("INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(node_id.to_string())
            .bind("test-node")
            .bind("127.0.0.1:50051")
            .bind("test-token")
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        let egg_id = Uuid::new_v4();
        sqlx::query("INSERT INTO eggs (id, name, start_cmd, created_at, updated_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(egg_id.to_string())
            .bind("test-egg")
            .bind("java -jar server.jar")
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        sqlx::query(
            "INSERT INTO egg_variables (id, egg_id, env_variable, name, user_viewable, user_editable, default_val)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(Uuid::new_v4().to_string())
        .bind(egg_id.to_string())
        .bind("EDITABLE_VAR")
        .bind("Editable Variable")
        .bind(true)
        .bind(true)
        .bind("default_value")
        .execute(&state.db)
        .await
        .unwrap();

        let server_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, env, egg_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(server_id.to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind("test-server")
        .bind("ubuntu:latest")
        .bind(2048)
        .bind(50)
        .bind(serde_json::to_string(&vec!["EDITABLE_VAR=default_value"]).unwrap())
        .bind(egg_id.to_string())
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .unwrap();

        let app = router(state.clone());
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/api/servers/{}/startup", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(
                Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "variables": {
                            "EDITABLE_VAR": "new_value"
                        }
                    }))
                    .unwrap(),
                ),
            )
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = res.into_body().collect().await.unwrap().to_bytes();
        let vars: Vec<StartupVariable> = serde_json::from_slice(&body).unwrap();

        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].env_variable, "EDITABLE_VAR");
        assert_eq!(vars[0].value, "new_value");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn test_docker_image_requires_admin(pool: sqlx::PgPool) {
        let (user_id, token) = seed_user(&pool, "u@t.com").await;
        let state = make_state(pool).await;

        let node_id = Uuid::new_v4();
        sqlx::query("INSERT INTO nodes (id, name, grpc_addr, token, created_at) VALUES ($1, $2, $3, $4, $5)")
            .bind(node_id.to_string())
            .bind("test-node")
            .bind("127.0.0.1:50051")
            .bind("test-token")
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&state.db)
            .await
            .unwrap();

        let server_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO servers (id, user_id, node_id, name, image, memory_mb, cpu_percent, env, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(server_id.to_string())
        .bind(user_id.to_string())
        .bind(node_id.to_string())
        .bind("test-server")
        .bind("ubuntu:latest")
        .bind(2048)
        .bind(50)
        .bind("[]")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .unwrap();

        let app = router(state.clone());
        let req = Request::builder()
            .method("PUT")
            .uri(format!("/api/servers/{}/startup/docker-image", server_id))
            .header("Authorization", format!("Bearer {}", token))
            .header("content-type", "application/json")
            .body(
                Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "image": "ubuntu:22.04"
                    }))
                    .unwrap(),
                ),
            )
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
    }
}
