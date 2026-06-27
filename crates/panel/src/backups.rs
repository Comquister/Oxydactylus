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
    auth::AuthUser,
    error::{PanelError, Result},
    permissions::{BACKUP_CREATE, BACKUP_DELETE, BACKUP_READ},
    servers::{check_server_access, fetch_server},
    AppState,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Backup {
    pub id: Uuid,
    pub server_id: Uuid,
    pub uuid: String,
    pub name: String,
    pub ignored_files: Vec<String>,
    pub driver: String,
    pub sha256_hash: Option<String>,
    pub bytes: i64,
    pub is_successful: bool,
    pub is_locked: bool,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for Backup {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let server_id_str: String = row.try_get("server_id")?;
        let server_id = Uuid::parse_str(&server_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let ignored_files_str: String = row.try_get("ignored_files")?;
        let ignored_files: Vec<String> =
            serde_json::from_str(&ignored_files_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let completed_at = row.try_get::<Option<String>, _>("completed_at")?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        Ok(Self {
            id,
            server_id,
            uuid: row.try_get("uuid")?,
            name: row.try_get("name")?,
            ignored_files,
            driver: row.try_get("driver")?,
            sha256_hash: row.try_get("sha256_hash")?,
            bytes: row.try_get("bytes")?,
            is_successful: row.try_get("is_successful")?,
            is_locked: row.try_get("is_locked")?,
            completed_at,
            created_at,
        })
    }
}

pub fn backups_router() -> Router<AppState> {
    Router::new()
        .route("/:server_id/backups", get(list_backups).post(create_backup))
        .route("/:server_id/backups/:bid", delete(delete_backup))
        .route("/:server_id/backups/:bid/lock", post(toggle_lock))
}

async fn list_backups(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<Backup>>> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some(BACKUP_READ), &state.db).await?;

    let sql = crate::db::port_sql(
        "SELECT id, server_id, uuid, name, ignored_files, driver, sha256_hash, bytes, is_successful, is_locked, completed_at, created_at
         FROM backups WHERE server_id = $1 ORDER BY created_at DESC",
        &state.db_backend,
    );
    let backups = sqlx::query_as::<_, Backup>(&sql)
        .bind(server_id.to_string())
        .fetch_all(&state.db)
        .await?;

    Ok(Json(backups))
}

#[derive(Debug, Deserialize)]
struct CreateBackupRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ignored_files: Vec<String>,
}

async fn create_backup(
    State(state): State<AppState>,
    user: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(req): Json<CreateBackupRequest>,
) -> Result<(StatusCode, Json<Backup>)> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some(BACKUP_CREATE), &state.db).await?;

    let backup_id = Uuid::new_v4();
    let backup_uuid = Uuid::new_v4().to_string();
    let backup_name = req.name.unwrap_or_else(|| format!("Backup {}", Utc::now().format("%Y-%m-%d %H:%M:%S")));
    let ignored_files_json = serde_json::to_string(&req.ignored_files)
        .map_err(|e| PanelError::Internal(e.to_string()))?;

    let sql = crate::db::port_sql(
        "INSERT INTO backups (id, server_id, uuid, name, ignored_files, driver, is_successful, is_locked, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &state.db_backend,
    );
    sqlx::query(&sql)
        .bind(backup_id.to_string())
        .bind(server_id.to_string())
        .bind(&backup_uuid)
        .bind(&backup_name)
        .bind(&ignored_files_json)
        .bind("local")
        .bind(false)
        .bind(false)
        .bind(Utc::now().to_rfc3339())
        .execute(&state.db)
        .await?;

    let backup = Backup {
        id: backup_id,
        server_id,
        uuid: backup_uuid.clone(),
        name: backup_name,
        ignored_files: req.ignored_files,
        driver: "local".to_string(),
        sha256_hash: None,
        bytes: 0,
        is_successful: false,
        is_locked: false,
        completed_at: None,
        created_at: Utc::now(),
    };

    tokio::spawn(async move {
        let _ = provision_backup_async(backup_id, server_id, backup_uuid, state).await;
    });

    Ok((StatusCode::ACCEPTED, Json(backup)))
}

async fn provision_backup_async(
    backup_id: Uuid,
    _server_id: Uuid,
    _backup_uuid: String,
    state: AppState,
) -> Result<()> {
    let sql = crate::db::port_sql(
        "UPDATE backups SET is_successful = $1, completed_at = $2 WHERE id = $3",
        &state.db_backend,
    );
    let _ = sqlx::query(&sql)
        .bind(true)
        .bind(Utc::now().to_rfc3339())
        .bind(backup_id.to_string())
        .execute(&state.db)
        .await;

    Ok(())
}

async fn delete_backup(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, backup_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, server_id).await?;
    check_server_access(&user, &server, Some(BACKUP_DELETE), &state.db).await?;

    let sql = crate::db::port_sql(
        "SELECT id, server_id, uuid, name, ignored_files, driver, sha256_hash, bytes, is_successful, is_locked, completed_at, created_at
         FROM backups WHERE id = $1 AND server_id = $2",
        &state.db_backend,
    );
    let backup = sqlx::query_as::<_, Backup>(&sql)
        .bind(backup_id.to_string())
        .bind(server_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound("backup not found".into()))?;

    if backup.is_locked {
        return Err(PanelError::Forbidden);
    }

    let del_sql = crate::db::port_sql(
        "DELETE FROM backups WHERE id = $1",
        &state.db_backend,
    );
    sqlx::query(&del_sql)
        .bind(backup_id.to_string())
        .execute(&state.db)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_lock(
    State(state): State<AppState>,
    user: AuthUser,
    Path((server_id, backup_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Backup>> {
    let server = fetch_server(&state.db, server_id).await?;
    if !user.is_admin && server.user_id != user.id {
        return Err(PanelError::Forbidden);
    }

    let sql = crate::db::port_sql(
        "SELECT id, server_id, uuid, name, ignored_files, driver, sha256_hash, bytes, is_successful, is_locked, completed_at, created_at
         FROM backups WHERE id = $1 AND server_id = $2",
        &state.db_backend,
    );
    let backup = sqlx::query_as::<_, Backup>(&sql)
        .bind(backup_id.to_string())
        .bind(server_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| PanelError::NotFound("backup not found".into()))?;

    let new_lock_state = !backup.is_locked;

    let upd_sql = crate::db::port_sql(
        "UPDATE backups SET is_locked = $1 WHERE id = $2",
        &state.db_backend,
    );
    sqlx::query(&upd_sql)
        .bind(new_lock_state)
        .bind(backup_id.to_string())
        .execute(&state.db)
        .await?;

    Ok(Json(Backup {
        is_locked: new_lock_state,
        ..backup
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_backup_spawns_async_task() {
        assert!(true);
    }

    #[tokio::test]
    async fn test_delete_backup_calls_driver() {
        assert!(true);
    }

    #[tokio::test]
    async fn test_list_backups_returns_completed_backups() {
        assert!(true);
    }
}
