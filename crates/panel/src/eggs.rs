use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
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

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct EggVariable {
    pub id:            Uuid,
    pub egg_id:        Uuid,
    pub name:          String,
    pub description:   Option<String>,
    pub env_variable:  String,
    pub default_val:   Option<String>,
    pub user_viewable: bool,
    pub user_editable: bool,
    pub rules:         Option<String>,
    pub field_type:    String,
}

#[derive(Debug, Deserialize)]
struct CreateVariableRequest {
    name:          String,
    #[serde(default)]
    description:   Option<String>,
    env_variable:  String,
    #[serde(default)]
    default_val:   Option<String>,
    #[serde(default = "bool_true")]
    user_viewable: bool,
    #[serde(default = "bool_true")]
    user_editable: bool,
    #[serde(default)]
    rules:         Option<String>,
    #[serde(default = "default_field_type")]
    field_type:    String,
}

fn bool_true()         -> bool   { true }
fn default_field_type() -> String { "text".to_string() }

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct InstallScript {
    pub id:         Uuid,
    pub egg_id:     Uuid,
    pub container:  String,
    pub entrypoint: String,
    pub script:     String,
}

#[derive(Debug, Deserialize)]
struct UpsertInstallScriptRequest {
    container:  String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    script:     String,
}

fn default_entrypoint() -> String { "bash".to_string() }

#[derive(Debug, sqlx::FromRow, Serialize)]
pub struct ConfigFile {
    pub id:      Uuid,
    pub egg_id:  Uuid,
    pub path:    String,
    pub parser:  String,
    pub patches: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct CreateConfigFileRequest {
    path:    String,
    parser:  String,
    patches: serde_json::Value,
}

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

async fn list_variables(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(egg_id): Path<Uuid>,
) -> Result<Json<Vec<EggVariable>>> {
    let vars = sqlx::query_as::<_, EggVariable>(
        "SELECT id, egg_id, name, description, env_variable, default_val,
                user_viewable, user_editable, rules, field_type
         FROM egg_variables WHERE egg_id = $1 ORDER BY name",
    )
    .bind(egg_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(vars))
}

async fn create_variable(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(egg_id): Path<Uuid>,
    Json(body): Json<CreateVariableRequest>,
) -> Result<(StatusCode, Json<EggVariable>)> {
    let var = sqlx::query_as::<_, EggVariable>(
        "INSERT INTO egg_variables
             (egg_id, name, description, env_variable, default_val,
              user_viewable, user_editable, rules, field_type)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         RETURNING id, egg_id, name, description, env_variable, default_val,
                   user_viewable, user_editable, rules, field_type",
    )
    .bind(egg_id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.env_variable)
    .bind(&body.default_val)
    .bind(body.user_viewable)
    .bind(body.user_editable)
    .bind(&body.rules)
    .bind(&body.field_type)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(var)))
}

async fn delete_variable(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((egg_id, var_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let rows = sqlx::query(
        "DELETE FROM egg_variables WHERE id = $1 AND egg_id = $2",
    )
    .bind(var_id)
    .bind(egg_id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(format!("variable {}", var_id)));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_install_script(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(egg_id): Path<Uuid>,
) -> Result<Json<InstallScript>> {
    let script = sqlx::query_as::<_, InstallScript>(
        "SELECT id, egg_id, container, entrypoint, script
         FROM egg_install_scripts WHERE egg_id = $1",
    )
    .bind(egg_id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(script))
}

async fn upsert_install_script(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(egg_id): Path<Uuid>,
    Json(body): Json<UpsertInstallScriptRequest>,
) -> Result<Json<InstallScript>> {
    let script = sqlx::query_as::<_, InstallScript>(
        "INSERT INTO egg_install_scripts (egg_id, container, entrypoint, script)
         VALUES ($1,$2,$3,$4)
         ON CONFLICT (egg_id)
         DO UPDATE SET container=$2, entrypoint=$3, script=$4
         RETURNING id, egg_id, container, entrypoint, script",
    )
    .bind(egg_id)
    .bind(&body.container)
    .bind(&body.entrypoint)
    .bind(&body.script)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(script))
}

async fn list_config_files(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(egg_id): Path<Uuid>,
) -> Result<Json<Vec<ConfigFile>>> {
    let cfs = sqlx::query_as::<_, ConfigFile>(
        "SELECT id, egg_id, path, parser, patches
         FROM egg_config_files WHERE egg_id = $1 ORDER BY path",
    )
    .bind(egg_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(cfs))
}

async fn create_config_file(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(egg_id): Path<Uuid>,
    Json(body): Json<CreateConfigFileRequest>,
) -> Result<(StatusCode, Json<ConfigFile>)> {
    let valid_parsers = ["properties","json","yaml","ini","xml"];
    if !valid_parsers.contains(&body.parser.as_str()) {
        return Err(PanelError::Validation(format!(
            "parser must be one of: {}", valid_parsers.join(", ")
        )));
    }
    let cf = sqlx::query_as::<_, ConfigFile>(
        "INSERT INTO egg_config_files (egg_id, path, parser, patches)
         VALUES ($1,$2,$3,$4)
         RETURNING id, egg_id, path, parser, patches",
    )
    .bind(egg_id)
    .bind(&body.path)
    .bind(&body.parser)
    .bind(&body.patches)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(cf)))
}

async fn delete_config_file(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((egg_id, cf_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let rows = sqlx::query(
        "DELETE FROM egg_config_files WHERE id = $1 AND egg_id = $2",
    )
    .bind(cf_id)
    .bind(egg_id)
    .execute(&state.db)
    .await?
    .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(format!("config-file {}", cf_id)));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn eggs_router() -> Router<AppState> {
    Router::new()
        .route("/",    get(list_eggs).post(create_egg))
        .route("/:id", get(get_egg).delete(delete_egg))
        .route("/:id/variables",              get(list_variables).post(create_variable))
        .route("/:id/variables/:var_id",      delete(delete_variable))
        .route("/:id/install-script",         get(get_install_script).put(upsert_install_script))
        .route("/:id/config-files",           get(list_config_files).post(create_config_file))
        .route("/:id/config-files/:cf_id",    delete(delete_config_file))
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

    #[sqlx::test(migrations = "./migrations")]
    async fn add_and_list_variables(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id: Uuid = sqlx::query_scalar(
            "INSERT INTO eggs (name, start_cmd, docker_images) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("e").bind("./run").bind(serde_json::json!({}))
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "name": "Memory",
            "env_variable": "MEMORY_MB",
            "default_val": "1024",
            "rules": "required|integer"
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("POST").uri(format!("/api/eggs/{}/variables", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let res2 = app.oneshot(
            Request::builder()
                .method("GET").uri(format!("/api/eggs/{}/variables", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes = res2.into_body().collect().await.unwrap().to_bytes();
        let vars: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(vars.as_array().unwrap().len(), 1);
        assert_eq!(vars[0]["env_variable"], "MEMORY_MB");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn set_and_get_install_script(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id: Uuid = sqlx::query_scalar(
            "INSERT INTO eggs (name, start_cmd, docker_images) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("e").bind("./run").bind(serde_json::json!({}))
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "container": "ghcr.io/ptero-eggs/installers:alpine",
            "entrypoint": "ash",
            "script": "#!/bin/ash\necho hello"
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("PUT").uri(format!("/api/eggs/{}/install-script", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res2 = app.oneshot(
            Request::builder()
                .method("GET").uri(format!("/api/eggs/{}/install-script", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes = res2.into_body().collect().await.unwrap().to_bytes();
        let got: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(got["entrypoint"], "ash");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn add_and_list_config_files(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id: Uuid = sqlx::query_scalar(
            "INSERT INTO eggs (name, start_cmd, docker_images) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind("e").bind("./run").bind(serde_json::json!({}))
        .fetch_one(&pool).await.unwrap();

        let app = router(make_state(pool));
        let body = serde_json::json!({
            "path": "server.properties",
            "parser": "properties",
            "patches": {"server-ip": "0.0.0.0", "server-port": "25565"}
        });
        let res = app.clone().oneshot(
            Request::builder()
                .method("POST").uri(format!("/api/eggs/{}/config-files", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let res2 = app.oneshot(
            Request::builder()
                .method("GET").uri(format!("/api/eggs/{}/config-files", egg_id))
                .header("authorization", format!("Bearer {}", token))
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes = res2.into_body().collect().await.unwrap().to_bytes();
        let cfs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(cfs.as_array().unwrap().len(), 1);
        assert_eq!(cfs[0]["path"], "server.properties");
    }
}
