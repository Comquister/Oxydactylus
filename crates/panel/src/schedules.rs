use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    db::port_sql,
    error::{PanelError, Result},
    servers::fetch_server,
    AppState,
};

#[derive(Debug, Serialize, Clone)]
pub struct ScheduleInfo {
    pub id: Uuid,
    pub server_id: Uuid,
    pub name: String,
    pub cron_minute: String,
    pub cron_hour: String,
    pub cron_day_of_month: String,
    pub cron_month: String,
    pub cron_day_of_week: String,
    pub is_active: bool,
    pub is_processing: bool,
    pub only_when_online: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for ScheduleInfo {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let server_id_str: String = row.try_get("server_id")?;
        let server_id = Uuid::parse_str(&server_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let last_run_at_opt: Option<String> = row.try_get("last_run_at")?;
        let last_run_at = last_run_at_opt.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        let next_run_at_opt: Option<String> = row.try_get("next_run_at")?;
        let next_run_at = next_run_at_opt.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            server_id,
            name: row.try_get("name")?,
            cron_minute: row.try_get("cron_minute")?,
            cron_hour: row.try_get("cron_hour")?,
            cron_day_of_month: row.try_get("cron_day_of_month")?,
            cron_month: row.try_get("cron_month")?,
            cron_day_of_week: row.try_get("cron_day_of_week")?,
            is_active: row.try_get("is_active")?,
            is_processing: row.try_get("is_processing")?,
            only_when_online: row.try_get("only_when_online")?,
            last_run_at,
            next_run_at,
            created_at,
        })
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ScheduleTask {
    pub id: Uuid,
    pub schedule_id: Uuid,
    pub sequence_id: i32,
    pub action: String,
    pub payload: String,
    pub time_offset: i32,
    pub is_queued: bool,
    pub continue_on_failure: bool,
    pub created_at: DateTime<Utc>,
}

impl<'r> sqlx::FromRow<'r, sqlx::any::AnyRow> for ScheduleTask {
    fn from_row(row: &'r sqlx::any::AnyRow) -> std::result::Result<Self, sqlx::Error> {
        use sqlx::Row;
        let id_str: String = row.try_get("id")?;
        let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;
        let schedule_id_str: String = row.try_get("schedule_id")?;
        let schedule_id = Uuid::parse_str(&schedule_id_str).map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        let created_at_str: String = row.try_get("created_at")?;
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| sqlx::Error::Decode(Box::new(e)))?;

        Ok(Self {
            id,
            schedule_id,
            sequence_id: row.try_get("sequence_id")?,
            action: row.try_get("action")?,
            payload: row.try_get("payload")?,
            time_offset: row.try_get("time_offset")?,
            is_queued: row.try_get("is_queued")?,
            continue_on_failure: row.try_get("continue_on_failure")?,
            created_at,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleRequest {
    pub name: String,
    pub cron_minute: Option<String>,
    pub cron_hour: Option<String>,
    pub cron_day_of_month: Option<String>,
    pub cron_month: Option<String>,
    pub cron_day_of_week: Option<String>,
    pub only_when_online: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleRequest {
    pub name: Option<String>,
    pub cron_minute: Option<String>,
    pub cron_hour: Option<String>,
    pub cron_day_of_month: Option<String>,
    pub cron_month: Option<String>,
    pub cron_day_of_week: Option<String>,
    pub is_active: Option<bool>,
    pub only_when_online: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleTaskRequest {
    pub action: String,
    pub payload: String,
    pub time_offset: Option<i32>,
    pub continue_on_failure: Option<bool>,
}

pub fn calculate_next_run_at(
    minute: &str,
    hour: &str,
    day_of_month: &str,
    month: &str,
    day_of_week: &str,
) -> Result<DateTime<Utc>> {
    let cron_expr = format!("0 {} {} {} {} {}", minute, hour, day_of_month, month, day_of_week);
    let schedule = Schedule::from_str(&cron_expr)
        .map_err(|_| PanelError::Validation("invalid cron expression".to_string()))?;
    schedule
        .upcoming(Utc)
        .next()
        .ok_or_else(|| PanelError::Validation("could not calculate next run".to_string()))
}

async fn list_schedules(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(server_id): Path<Uuid>,
) -> Result<Json<Vec<ScheduleInfo>>> {
    let server = fetch_server(&state.db, server_id).await?;
    if !caller.is_admin && caller.id != server.user_id {
        return Err(PanelError::Forbidden);
    }

    let sql = port_sql(
        "SELECT id, server_id, name, cron_minute, cron_hour, cron_day_of_month, cron_month, cron_day_of_week,
                is_active, is_processing, only_when_online, last_run_at, next_run_at, created_at
         FROM schedules WHERE server_id = $1 ORDER BY created_at",
        &state.db_backend,
    );
    let schedules = sqlx::query_as::<_, ScheduleInfo>(&sql)
        .bind(server_id.to_string())
        .fetch_all(&state.db)
        .await?;
    Ok(Json(schedules))
}

async fn create_schedule(
    State(state): State<AppState>,
    caller: AuthUser,
    Path(server_id): Path<Uuid>,
    Json(body): Json<CreateScheduleRequest>,
) -> Result<(StatusCode, Json<ScheduleInfo>)> {
    let server = fetch_server(&state.db, server_id).await?;
    if !caller.is_admin && caller.id != server.user_id {
        return Err(PanelError::Forbidden);
    }

    let minute = body.cron_minute.unwrap_or_else(|| "*".to_string());
    let hour = body.cron_hour.unwrap_or_else(|| "*".to_string());
    let day_of_month = body.cron_day_of_month.unwrap_or_else(|| "*".to_string());
    let month = body.cron_month.unwrap_or_else(|| "*".to_string());
    let day_of_week = body.cron_day_of_week.unwrap_or_else(|| "*".to_string());

    let next_run_at = calculate_next_run_at(&minute, &hour, &day_of_month, &month, &day_of_week)?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let next_run_at_str = next_run_at.to_rfc3339();
    let only_when_online = body.only_when_online.unwrap_or(false);

    let sql = port_sql(
        "INSERT INTO schedules (id, server_id, name, cron_minute, cron_hour, cron_day_of_month,
         cron_month, cron_day_of_week, is_active, is_processing, only_when_online, next_run_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         RETURNING id, server_id, name, cron_minute, cron_hour, cron_day_of_month, cron_month,
         cron_day_of_week, is_active, is_processing, only_when_online, last_run_at, next_run_at, created_at",
        &state.db_backend,
    );

    let schedule = sqlx::query_as::<_, ScheduleInfo>(&sql)
        .bind(&id)
        .bind(server_id.to_string())
        .bind(&body.name)
        .bind(&minute)
        .bind(&hour)
        .bind(&day_of_month)
        .bind(&month)
        .bind(&day_of_week)
        .bind(true)
        .bind(false)
        .bind(only_when_online)
        .bind(&next_run_at_str)
        .bind(&now)
        .fetch_one(&state.db)
        .await?;

    Ok((StatusCode::CREATED, Json(schedule)))
}

async fn delete_schedule(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((server_id, schedule_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, server_id).await?;
    if !caller.is_admin && caller.id != server.user_id {
        return Err(PanelError::Forbidden);
    }

    let sql = port_sql(
        "DELETE FROM schedules WHERE id = $1 AND server_id = $2",
        &state.db_backend,
    );
    let rows = sqlx::query(&sql)
        .bind(schedule_id.to_string())
        .bind(server_id.to_string())
        .execute(&state.db)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(PanelError::NotFound(schedule_id.to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_schedule_tasks(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((server_id, schedule_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<ScheduleTask>>> {
    let server = fetch_server(&state.db, server_id).await?;
    if !caller.is_admin && caller.id != server.user_id {
        return Err(PanelError::Forbidden);
    }

    let sql = port_sql(
        "SELECT t.id, t.schedule_id, t.sequence_id, t.action, t.payload, t.time_offset,
                t.is_queued, t.continue_on_failure, t.created_at
         FROM schedule_tasks t
         INNER JOIN schedules s ON t.schedule_id = s.id
         WHERE s.server_id = $1 AND t.schedule_id = $2
         ORDER BY t.sequence_id",
        &state.db_backend,
    );
    let tasks = sqlx::query_as::<_, ScheduleTask>(&sql)
        .bind(server_id.to_string())
        .bind(schedule_id.to_string())
        .fetch_all(&state.db)
        .await?;
    Ok(Json(tasks))
}

async fn create_schedule_task(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((server_id, schedule_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<CreateScheduleTaskRequest>,
) -> Result<(StatusCode, Json<ScheduleTask>)> {
    let server = fetch_server(&state.db, server_id).await?;
    if !caller.is_admin && caller.id != server.user_id {
        return Err(PanelError::Forbidden);
    }

    let max_seq_sql = port_sql(
        "SELECT MAX(sequence_id) as max_seq FROM schedule_tasks
         WHERE schedule_id = $1",
        &state.db_backend,
    );
    let seq_row: (Option<i32>,) = sqlx::query_as(&max_seq_sql)
        .bind(schedule_id.to_string())
        .fetch_optional(&state.db)
        .await?
        .unwrap_or((Some(0),));
    let sequence_id = seq_row.0.unwrap_or(0) + 1;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let time_offset = body.time_offset.unwrap_or(0);
    let continue_on_failure = body.continue_on_failure.unwrap_or(false);

    let sql = port_sql(
        "INSERT INTO schedule_tasks (id, schedule_id, sequence_id, action, payload, time_offset,
         is_queued, continue_on_failure, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, schedule_id, sequence_id, action, payload, time_offset, is_queued, continue_on_failure, created_at",
        &state.db_backend,
    );

    let task = sqlx::query_as::<_, ScheduleTask>(&sql)
        .bind(&id)
        .bind(schedule_id.to_string())
        .bind(sequence_id)
        .bind(&body.action)
        .bind(&body.payload)
        .bind(time_offset)
        .bind(false)
        .bind(continue_on_failure)
        .bind(&now)
        .fetch_one(&state.db)
        .await?;

    Ok((StatusCode::CREATED, Json(task)))
}

async fn delete_schedule_task(
    State(state): State<AppState>,
    caller: AuthUser,
    Path((server_id, schedule_id, task_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode> {
    let server = fetch_server(&state.db, server_id).await?;
    if !caller.is_admin && caller.id != server.user_id {
        return Err(PanelError::Forbidden);
    }

    let sql = port_sql(
        "DELETE FROM schedule_tasks WHERE id = $1 AND schedule_id = $2",
        &state.db_backend,
    );
    let rows = sqlx::query(&sql)
        .bind(task_id.to_string())
        .bind(schedule_id.to_string())
        .execute(&state.db)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(PanelError::NotFound(task_id.to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub fn schedules_router() -> Router<AppState> {
    Router::new()
        .route(
            "/:id/schedules",
            get(list_schedules).post(create_schedule),
        )
        .route(
            "/:id/schedules/:sid",
            delete(delete_schedule),
        )
        .route(
            "/:id/schedules/:sid/tasks",
            get(list_schedule_tasks).post(create_schedule_task),
        )
        .route(
            "/:id/schedules/:sid/tasks/:tid",
            delete(delete_schedule_task),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_expression_calculates_next_run() {
        let next_run = calculate_next_run_at("0", "12", "*", "*", "*").unwrap();
        let now = Utc::now();
        assert!(next_run > now);
    }

    #[test]
    fn test_cron_expression_invalid() {
        let result = calculate_next_run_at("99", "99", "99", "99", "99");
        assert!(result.is_err());
    }
}
