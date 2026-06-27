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

#[derive(Debug, Serialize)]
pub struct Egg {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: String,
    pub features: Vec<String>,
    pub file_denylist: Vec<String>,
    pub docker_images: serde_json::Value,
    pub start_cmd: String,
    pub stop_cmd: String,
    pub startup_done: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Egg {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let features_str: String = row.try_get("features")?;
        let features: Vec<String> = serde_json::from_str(&features_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let file_denylist_str: String = row.try_get("file_denylist")?;
        let file_denylist: Vec<String> = serde_json::from_str(&file_denylist_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let docker_images_str: String = row.try_get("docker_images")?;
        let docker_images: serde_json::Value = serde_json::from_str(&docker_images_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let updated_at_str: String = row.try_get("updated_at")?;
        let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            author: row.try_get("author")?,
            version: row.try_get("version")?,
            features,
            file_denylist,
            docker_images,
            start_cmd: row.try_get("start_cmd")?,
            stop_cmd: row.try_get("stop_cmd")?,
            startup_done: row.try_get("startup_done")?,
            created_at,
            updated_at,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateEggRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default = "default_version")]
    version: String,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    file_denylist: Vec<String>,
    #[serde(default = "empty_object")]
    docker_images: serde_json::Value,
    start_cmd: String,
    #[serde(default = "default_stop")]
    stop_cmd: String,
    #[serde(default)]
    startup_done: Option<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}
fn default_stop() -> String {
    "stop".to_string()
}
fn empty_object() -> serde_json::Value {
    serde_json::json!({})
}

#[derive(Debug, Serialize)]
pub struct EggVariable {
    pub id: Uuid,
    pub egg_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub env_variable: String,
    pub default_val: Option<String>,
    pub user_viewable: bool,
    pub user_editable: bool,
    pub rules: Option<String>,
    pub field_type: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for EggVariable {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let egg_id_str: String = row.try_get("egg_id")?;
        let egg_id = Uuid::parse_str(&egg_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Self {
            id,
            egg_id,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            env_variable: row.try_get("env_variable")?,
            default_val: row.try_get("default_val")?,
            user_viewable: row.try_get("user_viewable")?,
            user_editable: row.try_get("user_editable")?,
            rules: row.try_get("rules")?,
            field_type: row.try_get("field_type")?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateVariableRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    env_variable: String,
    #[serde(default)]
    default_val: Option<String>,
    #[serde(default = "bool_true")]
    user_viewable: bool,
    #[serde(default = "bool_true")]
    user_editable: bool,
    #[serde(default)]
    rules: Option<String>,
    #[serde(default = "default_field_type")]
    field_type: String,
}

fn bool_true() -> bool {
    true
}
fn default_field_type() -> String {
    "text".to_string()
}

#[derive(Debug, Serialize)]
pub struct InstallScript {
    pub id: Uuid,
    pub egg_id: Uuid,
    pub container: String,
    pub entrypoint: String,
    pub script: String,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for InstallScript {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let egg_id_str: String = row.try_get("egg_id")?;
        let egg_id = Uuid::parse_str(&egg_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Self {
            id,
            egg_id,
            container: row.try_get("container")?,
            entrypoint: row.try_get("entrypoint")?,
            script: row.try_get("script")?,
        })
    }
}

#[derive(Debug, Deserialize)]
struct UpsertInstallScriptRequest {
    container: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
    script: String,
}

fn default_entrypoint() -> String {
    "bash".to_string()
}

#[derive(Debug, Serialize)]
pub struct ConfigFile {
    pub id: Uuid,
    pub egg_id: Uuid,
    pub path: String,
    pub parser: String,
    pub patches: serde_json::Value,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for ConfigFile {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let egg_id_str: String = row.try_get("egg_id")?;
        let egg_id = Uuid::parse_str(&egg_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let patches_str: String = row.try_get("patches")?;
        let patches: serde_json::Value = serde_json::from_str(&patches_str)
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        Ok(Self {
            id,
            egg_id,
            path: row.try_get("path")?,
            parser: row.try_get("parser")?,
            patches,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateConfigFileRequest {
    path: String,
    parser: String,
    patches: serde_json::Value,
}

async fn list_eggs(State(state): State<AppState>, _user: AuthUser) -> Result<Json<Vec<Egg>>> {
    let sql = crate::db::port_sql(
        "SELECT id, name, description, author, version, features, file_denylist,
                docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at
         FROM eggs ORDER BY created_at",
        &state.db_backend,
    );
    let eggs = sqlx::query_as::<_, Egg>(&sql)
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
        return Err(PanelError::Validation(
            "name and start_cmd are required".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let sql = crate::db::port_sql(
        "INSERT INTO eggs
             (id, name, description, author, version, features, file_denylist,
              docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
         RETURNING id, name, description, author, version, features, file_denylist,
                   docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at",
        &state.db_backend,
    );
    let egg = sqlx::query_as::<_, Egg>(&sql)
        .bind(&id)
        .bind(&body.name)
        .bind(&body.description)
        .bind(&body.author)
        .bind(&body.version)
        .bind(serde_json::to_string(&body.features).unwrap())
        .bind(serde_json::to_string(&body.file_denylist).unwrap())
        .bind(serde_json::to_string(&body.docker_images).unwrap())
        .bind(&body.start_cmd)
        .bind(&body.stop_cmd)
        .bind(&body.startup_done)
        .bind(&now)
        .bind(&now)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(egg)))
}

async fn get_egg(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Egg>> {
    let sql = crate::db::port_sql(
        "SELECT id, name, description, author, version, features, file_denylist,
                docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at
         FROM eggs WHERE id = $1",
        &state.db_backend,
    );
    let egg = sqlx::query_as::<_, Egg>(&sql)
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;
    Ok(Json(egg))
}

async fn delete_egg(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode> {
    let sql = crate::db::port_sql(
        "DELETE FROM eggs WHERE id = $1",
        &state.db_backend,
    );
    let rows = sqlx::query(&sql)
        .bind(id.to_string())
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
    let sql = crate::db::port_sql(
        "SELECT id, egg_id, name, description, env_variable, default_val,
                user_viewable, user_editable, rules, field_type
         FROM egg_variables WHERE egg_id = $1 ORDER BY name",
        &state.db_backend,
    );
    let vars = sqlx::query_as::<_, EggVariable>(&sql)
        .bind(egg_id.to_string())
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
    let id = Uuid::new_v4().to_string();
    let sql = crate::db::port_sql(
        "INSERT INTO egg_variables
             (id, egg_id, name, description, env_variable, default_val,
              user_viewable, user_editable, rules, field_type)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING id, egg_id, name, description, env_variable, default_val,
                   user_viewable, user_editable, rules, field_type",
        &state.db_backend,
    );
    let var = sqlx::query_as::<_, EggVariable>(&sql)
        .bind(&id)
        .bind(egg_id.to_string())
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
    let sql = crate::db::port_sql(
        "DELETE FROM egg_variables WHERE id = $1 AND egg_id = $2",
        &state.db_backend,
    );
    let rows = sqlx::query(&sql)
        .bind(var_id.to_string())
        .bind(egg_id.to_string())
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
    let sql = crate::db::port_sql(
        "SELECT id, egg_id, container, entrypoint, script
         FROM egg_install_scripts WHERE egg_id = $1",
        &state.db_backend,
    );
    let script = sqlx::query_as::<_, InstallScript>(&sql)
        .bind(egg_id.to_string())
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
    let id = Uuid::new_v4().to_string();
    let sql = crate::db::port_sql(
        "INSERT INTO egg_install_scripts (id, egg_id, container, entrypoint, script)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (egg_id)
         DO UPDATE SET container=$3, entrypoint=$4, script=$5
         RETURNING id, egg_id, container, entrypoint, script",
        &state.db_backend,
    );
    let script = sqlx::query_as::<_, InstallScript>(&sql)
        .bind(&id)
        .bind(egg_id.to_string())
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
    let sql = crate::db::port_sql(
        "SELECT id, egg_id, path, parser, patches
         FROM egg_config_files WHERE egg_id = $1 ORDER BY path",
        &state.db_backend,
    );
    let cfs = sqlx::query_as::<_, ConfigFile>(&sql)
        .bind(egg_id.to_string())
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
    let valid_parsers = ["properties", "json", "yaml", "ini", "xml"];
    if !valid_parsers.contains(&body.parser.as_str()) {
        return Err(PanelError::Validation(format!(
            "parser must be one of: {}",
            valid_parsers.join(", ")
        )));
    }
    let id = Uuid::new_v4().to_string();
    let sql = crate::db::port_sql(
        "INSERT INTO egg_config_files (id, egg_id, path, parser, patches)
         VALUES ($1,$2,$3,$4,$5)
         RETURNING id, egg_id, path, parser, patches",
        &state.db_backend,
    );
    let cf = sqlx::query_as::<_, ConfigFile>(&sql)
        .bind(&id)
        .bind(egg_id.to_string())
        .bind(&body.path)
        .bind(&body.parser)
        .bind(serde_json::to_string(&body.patches).unwrap())
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(cf)))
}

async fn delete_config_file(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path((egg_id, cf_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let sql = crate::db::port_sql(
        "DELETE FROM egg_config_files WHERE id = $1 AND egg_id = $2",
        &state.db_backend,
    );
    let rows = sqlx::query(&sql)
        .bind(cf_id.to_string())
        .bind(egg_id.to_string())
        .execute(&state.db)
        .await?
        .rows_affected();
    if rows == 0 {
        return Err(PanelError::NotFound(format!("config-file {}", cf_id)));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ── PTDL_v2 import ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct Ptdlv2 {
    name: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    file_denylist: Vec<String>,
    #[serde(default)]
    docker_images: serde_json::Value,
    startup: String,
    config: Ptdlv2Config,
    #[serde(default)]
    variables: Vec<Ptdlv2Variable>,
    #[serde(default)]
    scripts: Option<Ptdlv2Scripts>,
}

#[derive(Debug, Deserialize, Default)]
struct Ptdlv2Config {
    #[serde(default = "default_stop")]
    stop: String,
    #[serde(default)]
    startup: Ptdlv2Startup,
    #[serde(default)]
    files: String,
}

#[derive(Debug, Deserialize, Default)]
struct Ptdlv2Startup {
    #[serde(default)]
    done: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Ptdlv2Variable {
    name: String,
    env_variable: String,
    #[serde(default)]
    default_value: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "bool_true")]
    user_viewable: bool,
    #[serde(default = "bool_true")]
    user_editable: bool,
    #[serde(default)]
    rules: Option<String>,
    #[serde(default = "default_field_type")]
    field_type: String,
}

#[derive(Debug, Deserialize, Default)]
struct Ptdlv2Scripts {
    installation: Option<Ptdlv2InstallScript>,
}

#[derive(Debug, Deserialize)]
struct Ptdlv2InstallScript {
    script: String,
    container: String,
    #[serde(default = "default_entrypoint")]
    entrypoint: String,
}

async fn import_egg(
    State(state): State<AppState>,
    _admin: AdminUser,
    Json(body): Json<Ptdlv2>,
) -> Result<(StatusCode, Json<Egg>)> {
    let docker_images = if body.docker_images.is_null() {
        serde_json::json!({})
    } else {
        body.docker_images
    };

    let egg_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let sql = crate::db::port_sql(
        "INSERT INTO eggs
             (id, name, description, author, features, file_denylist,
              docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
         RETURNING id, name, description, author, version, features, file_denylist,
                   docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at",
        &state.db_backend,
    );
    let egg = sqlx::query_as::<_, Egg>(&sql)
        .bind(&egg_id)
        .bind(&body.name)
        .bind(&body.description)
        .bind(&body.author)
        .bind(serde_json::to_string(&body.features).unwrap())
        .bind(serde_json::to_string(&body.file_denylist).unwrap())
        .bind(serde_json::to_string(&docker_images).unwrap())
        .bind(&body.startup)
        .bind(&body.config.stop)
        .bind(&body.config.startup.done)
        .bind(&now)
        .bind(&now)
        .fetch_one(&state.db)
        .await?;

    for var in &body.variables {
        let var_id = Uuid::new_v4().to_string();
        let sql = crate::db::port_sql(
            "INSERT INTO egg_variables
                  (id, egg_id, name, description, env_variable, default_val,
                   user_viewable, user_editable, rules, field_type)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
            &state.db_backend,
        );
        sqlx::query(&sql)
            .bind(&var_id)
            .bind(egg.id.to_string())
            .bind(&var.name)
            .bind(&var.description)
            .bind(&var.env_variable)
            .bind(&var.default_value)
            .bind(var.user_viewable)
            .bind(var.user_editable)
            .bind(&var.rules)
            .bind(&var.field_type)
            .execute(&state.db)
            .await?;
    }

    if let Some(scripts) = &body.scripts {
        if let Some(install) = &scripts.installation {
            let script_id = Uuid::new_v4().to_string();
            let sql = crate::db::port_sql(
                "INSERT INTO egg_install_scripts (id, egg_id, container, entrypoint, script)
                 VALUES ($1,$2,$3,$4,$5)
                 ON CONFLICT (egg_id) DO UPDATE
                 SET container=$3, entrypoint=$4, script=$5",
                &state.db_backend,
            );
            sqlx::query(&sql)
                .bind(&script_id)
                .bind(egg.id.to_string())
                .bind(&install.container)
                .bind(&install.entrypoint)
                .bind(&install.script)
                .execute(&state.db)
                .await?;
        }
    }

    Ok((StatusCode::CREATED, Json(egg)))
}

// ── .toml export ─────────────────────────────────────────────────────────────

use axum::http::header;
use axum::response::Response as AxumResponse;

async fn export_egg_toml(
    State(state): State<AppState>,
    _user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<AxumResponse> {
    let sql_egg = crate::db::port_sql(
        "SELECT id, name, description, author, version, features, file_denylist,
                docker_images, start_cmd, stop_cmd, startup_done, created_at, updated_at
         FROM eggs WHERE id = $1",
        &state.db_backend,
    );
    let egg = sqlx::query_as::<_, Egg>(&sql_egg)
        .bind(id.to_string())
        .fetch_one(&state.db)
        .await?;

    let sql_vars = crate::db::port_sql(
        "SELECT id, egg_id, name, description, env_variable, default_val,
                user_viewable, user_editable, rules, field_type
         FROM egg_variables WHERE egg_id = $1 ORDER BY name",
        &state.db_backend,
    );
    let vars = sqlx::query_as::<_, EggVariable>(&sql_vars)
        .bind(id.to_string())
        .fetch_all(&state.db)
        .await?;

    let sql_install = crate::db::port_sql(
        "SELECT id, egg_id, container, entrypoint, script
         FROM egg_install_scripts WHERE egg_id = $1",
        &state.db_backend,
    );
    let install = sqlx::query_as::<_, InstallScript>(&sql_install)
        .bind(id.to_string())
        .fetch_optional(&state.db)
        .await?;

    let sql_cfs = crate::db::port_sql(
        "SELECT id, egg_id, path, parser, patches
         FROM egg_config_files WHERE egg_id = $1 ORDER BY path",
        &state.db_backend,
    );
    let cfs = sqlx::query_as::<_, ConfigFile>(&sql_cfs)
        .bind(id.to_string())
        .fetch_all(&state.db)
        .await?;

    let toml_str = build_egg_toml(&egg, &vars, install.as_ref(), &cfs)
        .map_err(|e| PanelError::Internal(e.to_string()))?;

    let response = axum::http::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/toml; charset=utf-8")
        .body(axum::body::Body::from(toml_str))
        .map_err(|e| PanelError::Internal(e.to_string()))?;
    Ok(response)
}

fn build_egg_toml(
    egg: &Egg,
    vars: &[EggVariable],
    install: Option<&InstallScript>,
    cfs: &[ConfigFile],
) -> std::result::Result<String, toml::ser::Error> {
    let mut doc = toml::Table::new();

    // [egg]
    let mut egg_tbl = toml::Table::new();
    egg_tbl.insert("name".into(), toml::Value::String(egg.name.clone()));
    if let Some(a) = &egg.author {
        egg_tbl.insert("author".into(), toml::Value::String(a.clone()));
    }
    if let Some(d) = &egg.description {
        egg_tbl.insert("description".into(), toml::Value::String(d.clone()));
    }
    egg_tbl.insert(
        "features".into(),
        toml::Value::Array(
            egg.features
                .iter()
                .map(|f| toml::Value::String(f.clone()))
                .collect(),
        ),
    );
    egg_tbl.insert(
        "file_denylist".into(),
        toml::Value::Array(
            egg.file_denylist
                .iter()
                .map(|f| toml::Value::String(f.clone()))
                .collect(),
        ),
    );
    doc.insert("egg".into(), toml::Value::Table(egg_tbl));

    // [startup]
    let mut startup_tbl = toml::Table::new();
    startup_tbl.insert("command".into(), toml::Value::String(egg.start_cmd.clone()));
    startup_tbl.insert("stop".into(), toml::Value::String(egg.stop_cmd.clone()));
    if let Some(d) = &egg.startup_done {
        startup_tbl.insert("detection".into(), toml::Value::String(d.clone()));
    }
    doc.insert("startup".into(), toml::Value::Table(startup_tbl));

    // [docker_images]
    if let serde_json::Value::Object(map) = &egg.docker_images {
        let mut di = toml::Table::new();
        for (k, v) in map {
            if let serde_json::Value::String(s) = v {
                di.insert(k.clone(), toml::Value::String(s.clone()));
            }
        }
        doc.insert("docker_images".into(), toml::Value::Table(di));
    }

    // [[variables]]
    if !vars.is_empty() {
        let var_arr: Vec<toml::Value> = vars
            .iter()
            .map(|v| {
                let mut t = toml::Table::new();
                t.insert("name".into(), toml::Value::String(v.name.clone()));
                t.insert(
                    "env_variable".into(),
                    toml::Value::String(v.env_variable.clone()),
                );
                if let Some(d) = &v.default_val {
                    t.insert("default".into(), toml::Value::String(d.clone()));
                }
                if let Some(d) = &v.description {
                    t.insert("description".into(), toml::Value::String(d.clone()));
                }
                if let Some(r) = &v.rules {
                    t.insert("rules".into(), toml::Value::String(r.clone()));
                }
                t.insert(
                    "user_viewable".into(),
                    toml::Value::Boolean(v.user_viewable),
                );
                t.insert(
                    "user_editable".into(),
                    toml::Value::Boolean(v.user_editable),
                );
                t.insert(
                    "field_type".into(),
                    toml::Value::String(v.field_type.clone()),
                );
                toml::Value::Table(t)
            })
            .collect();
        doc.insert("variables".into(), toml::Value::Array(var_arr));
    }

    // [install]
    if let Some(i) = install {
        let mut inst = toml::Table::new();
        inst.insert("container".into(), toml::Value::String(i.container.clone()));
        inst.insert(
            "entrypoint".into(),
            toml::Value::String(i.entrypoint.clone()),
        );
        inst.insert("script".into(), toml::Value::String(i.script.clone()));
        doc.insert("install".into(), toml::Value::Table(inst));
    }

    // [[config_files]]
    if !cfs.is_empty() {
        let cf_arr: Vec<toml::Value> = cfs
            .iter()
            .map(|cf| {
                let mut t = toml::Table::new();
                t.insert("path".into(), toml::Value::String(cf.path.clone()));
                t.insert("parser".into(), toml::Value::String(cf.parser.clone()));
                if let serde_json::Value::Object(patches) = &cf.patches {
                    let mut p = toml::Table::new();
                    for (k, v) in patches {
                        if let serde_json::Value::String(s) = v {
                            p.insert(k.clone(), toml::Value::String(s.clone()));
                        }
                    }
                    t.insert("patches".into(), toml::Value::Table(p));
                }
                toml::Value::Table(t)
            })
            .collect();
        doc.insert("config_files".into(), toml::Value::Array(cf_arr));
    }

    toml::to_string_pretty(&doc)
}

pub fn eggs_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_eggs).post(create_egg))
        .route("/import", post(import_egg))
        .route("/:id", get(get_egg).delete(delete_egg))
        .route("/:id/export", get(export_egg_toml))
        .route("/:id/variables", get(list_variables).post(create_variable))
        .route("/:id/variables/:var_id", delete(delete_variable))
        .route(
            "/:id/install-script",
            get(get_install_script).put(upsert_install_script),
        )
        .route(
            "/:id/config-files",
            get(list_config_files).post(create_config_file),
        )
        .route("/:id/config-files/:cf_id", delete(delete_config_file))
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

    async fn seed_admin(pool: &sqlx::PgPool) -> String {
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
        encode_token(id, true, "access", SECRET, 900).unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_eggs_empty(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool).await);
        let req = Request::builder()
            .method("GET")
            .uri("/api/eggs")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();
        let res = app.oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_and_get_egg(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "name": "Purpur",
            "start_cmd": "java -jar server.jar",
            "stop_cmd": "stop",
            "docker_images": {"Java 21": "ghcr.io/ptero-eggs/yolks:java_21"}
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/eggs")
                    .header("authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let egg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let egg_id = egg["id"].as_str().unwrap();

        let res2 = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/eggs/{}", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes2 = res2.into_body().collect().await.unwrap().to_bytes();
        let got: serde_json::Value = serde_json::from_slice(&bytes2).unwrap();
        assert_eq!(got["name"], "Purpur");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn delete_egg(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id_str: String = sqlx::query_scalar(
            "INSERT INTO eggs (id, name, start_cmd, docker_images, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("test-egg")
        .bind("./run.sh")
        .bind(serde_json::to_string(&serde_json::json!({})).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let egg_id = Uuid::parse_str(&egg_id_str).unwrap();

        let app = router(make_state(pool).await);
        let res = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/eggs/{}", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn add_and_list_variables(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id_str: String = sqlx::query_scalar(
            "INSERT INTO eggs (id, name, start_cmd, docker_images, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("e")
        .bind("./run")
        .bind(serde_json::to_string(&serde_json::json!({})).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let egg_id = Uuid::parse_str(&egg_id_str).unwrap();

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "name": "Memory",
            "env_variable": "MEMORY_MB",
            "default_val": "1024",
            "rules": "required|integer"
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/eggs/{}/variables", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let res2 = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/eggs/{}/variables", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes = res2.into_body().collect().await.unwrap().to_bytes();
        let vars: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(vars.as_array().unwrap().len(), 1);
        assert_eq!(vars[0]["env_variable"], "MEMORY_MB");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn set_and_get_install_script(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id_str: String = sqlx::query_scalar(
            "INSERT INTO eggs (id, name, start_cmd, docker_images, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("e")
        .bind("./run")
        .bind(serde_json::to_string(&serde_json::json!({})).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let egg_id = Uuid::parse_str(&egg_id_str).unwrap();

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "container": "ghcr.io/ptero-eggs/installers:alpine",
            "entrypoint": "ash",
            "script": "#!/bin/ash\necho hello"
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(format!("/api/eggs/{}/install-script", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let res2 = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/eggs/{}/install-script", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes = res2.into_body().collect().await.unwrap().to_bytes();
        let got: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(got["entrypoint"], "ash");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn add_and_list_config_files(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id_str: String = sqlx::query_scalar(
            "INSERT INTO eggs (id, name, start_cmd, docker_images, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("e")
        .bind("./run")
        .bind(serde_json::to_string(&serde_json::json!({})).unwrap())
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let egg_id = Uuid::parse_str(&egg_id_str).unwrap();

        let app = router(make_state(pool).await);
        let body = serde_json::json!({
            "path": "server.properties",
            "parser": "properties",
            "patches": {"server-ip": "0.0.0.0", "server-port": "25565"}
        });
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/eggs/{}/config-files", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);

        let res2 = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/eggs/{}/config-files", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res2.status(), StatusCode::OK);
        let bytes = res2.into_body().collect().await.unwrap().to_bytes();
        let cfs: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(cfs.as_array().unwrap().len(), 1);
        assert_eq!(cfs[0]["path"], "server.properties");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn import_ptdl_v2(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let app = router(make_state(pool.clone()).await);

        let ptdl = serde_json::json!({
            "name": "Purpur",
            "author": "purpur@birdflop.com",
            "description": "Purpur MC server",
            "features": ["eula"],
            "file_denylist": [],
            "docker_images": {"Java 21": "ghcr.io/ptero-eggs/yolks:java_21"},
            "startup": "java {{JVM_EXTRA}} -jar {{SERVER_JARFILE}}",
            "config": {
                "stop": "stop",
                "startup": {"done": "For help, type"},
                "files": "{}"
            },
            "variables": [
                {
                    "name": "Server Jar File",
                    "env_variable": "SERVER_JARFILE",
                    "default_value": "server.jar",
                    "description": "The jar file to run",
                    "user_viewable": true,
                    "user_editable": true,
                    "rules": "required|string|max:80",
                    "field_type": "text"
                }
            ],
            "scripts": {
                "installation": {
                    "script": "#!/bin/ash\necho hi",
                    "container": "ghcr.io/ptero-eggs/installers:alpine",
                    "entrypoint": "ash"
                }
            }
        });

        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/eggs/import")
                    .header("authorization", format!("Bearer {}", token))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&ptdl).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let egg: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(egg["name"], "Purpur");
        assert_eq!(egg["stop_cmd"], "stop");
        assert_eq!(egg["startup_done"], "For help, type");

        // variable was imported
        let var_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM egg_variables WHERE egg_id = $1")
                .bind(egg["id"].as_str().unwrap().to_string())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(var_count, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn export_toml_roundtrip(pool: sqlx::PgPool) {
        let token = seed_admin(&pool).await;
        let egg_id_str: String = sqlx::query_scalar(
            "INSERT INTO eggs (id, name, start_cmd, stop_cmd, docker_images, startup_done, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind("TestEgg")
        .bind("./run")
        .bind("stop")
        .bind(serde_json::to_string(&serde_json::json!({"Java 21": "ghcr.io/test:java_21"})).unwrap())
        .bind("Server started")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&pool)
        .await
        .unwrap();
        let egg_id = Uuid::parse_str(&egg_id_str).unwrap();

        sqlx::query(
            "INSERT INTO egg_variables (id, egg_id, name, env_variable, default_val, rules, field_type)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(egg_id.to_string())
        .bind("Port")
        .bind("PORT")
        .bind("25565")
        .bind("required|integer")
        .bind("text")
        .execute(&pool)
        .await
        .unwrap();

        let app = router(make_state(pool).await);
        let res = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/eggs/{}/export", egg_id))
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/toml"), "expected toml, got {}", ct);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(body.contains("TestEgg"));
        assert!(body.contains("PORT"));
    }
}
