use chrono::Utc;
use cron::Schedule;
use serde_json::json;
use sqlx::AnyPool;
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

use crate::activity::{ActivityEntry, log_activity};
use crate::db::port_sql;
use crate::node_client::NodeClient;
use crate::servers::fetch_server;

#[derive(sqlx::FromRow)]
struct NodeRow {
    grpc_addr: String,
    token: String,
}

pub fn start(pool: AnyPool, backend: String) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let _ = tick(&pool, &backend).await;
        }
    });
}

async fn tick(pool: &AnyPool, backend: &str) -> Result<(), String> {
    let now = Utc::now();
    let now_str = now.to_rfc3339();

    let sql = port_sql(
        "SELECT id, server_id, cron_minute, cron_hour, cron_day_of_month, cron_month, cron_day_of_week,
                only_when_online
         FROM schedules
         WHERE is_active = TRUE
         AND is_processing = FALSE
         AND next_run_at IS NOT NULL
         AND next_run_at <= $1",
        backend,
    );

    #[derive(sqlx::FromRow)]
    struct PendingSchedule {
        id: String,
        server_id: String,
        cron_minute: String,
        cron_hour: String,
        cron_day_of_month: String,
        cron_month: String,
        cron_day_of_week: String,
        only_when_online: bool,
    }

    let schedules = sqlx::query_as::<_, PendingSchedule>(&sql)
        .bind(&now_str)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    for schedule in schedules {
        let _ = run_schedule(
            pool,
            backend,
            &schedule.id,
            &schedule.server_id,
            &schedule.cron_minute,
            &schedule.cron_hour,
            &schedule.cron_day_of_month,
            &schedule.cron_month,
            &schedule.cron_day_of_week,
            schedule.only_when_online,
        ).await;
    }

    Ok(())
}

async fn run_schedule(
    pool: &AnyPool,
    backend: &str,
    schedule_id: &str,
    server_id: &str,
    minute: &str,
    hour: &str,
    day_of_month: &str,
    month: &str,
    day_of_week: &str,
    only_when_online: bool,
) -> Result<(), String> {
    let set_processing_sql = port_sql(
        "UPDATE schedules SET is_processing = TRUE WHERE id = $1",
        backend,
    );
    sqlx::query(&set_processing_sql)
        .bind(schedule_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    let server_uuid = Uuid::parse_str(server_id).map_err(|e| e.to_string())?;
    let server = match fetch_server(pool, server_uuid).await {
        Ok(s) => s,
        Err(_) => {
            let _ = clear_processing(pool, backend, schedule_id).await;
            return Ok(());
        }
    };

    if only_when_online && server.status != "running" {
        let _ = clear_processing(pool, backend, schedule_id).await;
        return Ok(());
    }

    let get_tasks_sql = port_sql(
        "SELECT id, action, payload, time_offset, continue_on_failure
         FROM schedule_tasks
         WHERE schedule_id = $1
         ORDER BY sequence_id",
        backend,
    );

    #[derive(sqlx::FromRow)]
    struct TaskRow {
        id: String,
        action: String,
        payload: String,
        time_offset: i32,
        continue_on_failure: bool,
    }

    let tasks = sqlx::query_as::<_, TaskRow>(&get_tasks_sql)
        .bind(schedule_id)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;

    let mut failed = false;
    for task in tasks {
        if failed && !task.continue_on_failure {
            break;
        }

        tokio::time::sleep(Duration::from_secs(task.time_offset as u64)).await;

        match run_task(pool, backend, &server_uuid, &task.action, &task.payload).await {
            Ok(_) => {
                log_activity(
                    pool.clone(),
                    backend.to_string(),
                    ActivityEntry {
                        server_id: Some(server_uuid),
                        user_id: None,
                        event: "schedule.run".to_string(),
                        properties: json!({
                            "schedule_id": schedule_id,
                            "action": task.action,
                            "success": true
                        }),
                        ip: None,
                    },
                ).await;
            }
            Err(e) => {
                failed = true;
                log_activity(
                    pool.clone(),
                    backend.to_string(),
                    ActivityEntry {
                        server_id: Some(server_uuid),
                        user_id: None,
                        event: "schedule.run".to_string(),
                        properties: json!({
                            "schedule_id": schedule_id,
                            "action": task.action,
                            "success": false,
                            "error": e.to_string()
                        }),
                        ip: None,
                    },
                ).await;
            }
        }
    }

    let cron_expr = format!("0 {} {} {} {} {}", minute, hour, day_of_month, month, day_of_week);
    let next_run_at = if let Ok(schedule) = Schedule::from_str(&cron_expr) {
        schedule
            .upcoming(Utc)
            .next()
            .map(|dt| dt.to_rfc3339())
    } else {
        None
    };

    let last_run_at = Utc::now().to_rfc3339();
    let update_sql = port_sql(
        "UPDATE schedules SET is_processing = FALSE, last_run_at = $1, next_run_at = $2 WHERE id = $3",
        backend,
    );
    sqlx::query(&update_sql)
        .bind(&last_run_at)
        .bind(next_run_at)
        .bind(schedule_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

async fn run_task(
    pool: &AnyPool,
    backend: &str,
    server_id: &Uuid,
    action: &str,
    payload: &str,
) -> Result<(), String> {
    let server = fetch_server(pool, *server_id).await.map_err(|e| e.to_string())?;

    let node_sql = port_sql(
        "SELECT grpc_addr, token FROM nodes WHERE id = $1",
        backend,
    );
    let node: NodeRow = sqlx::query_as(&node_sql)
        .bind(server.node_id.to_string())
        .fetch_one(pool)
        .await
        .map_err(|e| e.to_string())?;

    match action {
        "power" => {
            let power_action: String = serde_json::from_str(payload).map_err(|e| e.to_string())?;
            match power_action.as_str() {
                "start" => {
                    let mut client = NodeClient::connect(&node.grpc_addr, &node.token).await.map_err(|e| e.to_string())?;
                    client.start(&server_id.to_string()).await.map_err(|e| e.to_string())?;
                }
                "stop" => {
                    let mut client = NodeClient::connect(&node.grpc_addr, &node.token).await.map_err(|e| e.to_string())?;
                    client.stop(&server_id.to_string(), 10).await.map_err(|e| e.to_string())?;
                }
                "restart" => {
                    let mut client = NodeClient::connect(&node.grpc_addr, &node.token).await.map_err(|e| e.to_string())?;
                    client.stop(&server_id.to_string(), 10).await.map_err(|e| e.to_string())?;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    let mut client = NodeClient::connect(&node.grpc_addr, &node.token).await.map_err(|e| e.to_string())?;
                    client.start(&server_id.to_string()).await.map_err(|e| e.to_string())?;
                }
                _ => return Err("unknown power action".into()),
            }
        }
        "command" => {
            let command: String = serde_json::from_str(payload).map_err(|e| e.to_string())?;
            let mut client = NodeClient::connect(&node.grpc_addr, &node.token).await.map_err(|e| e.to_string())?;
            client.send_command(&server_id.to_string(), &command).await.map_err(|e| e.to_string())?;
        }
        "backup" => {
            return Err("backup action not yet implemented".into());
        }
        _ => return Err(format!("unknown action: {}", action).into()),
    }

    Ok(())
}

async fn clear_processing(
    pool: &AnyPool,
    backend: &str,
    schedule_id: &str,
) -> Result<(), String> {
    let sql = port_sql(
        "UPDATE schedules SET is_processing = FALSE WHERE id = $1",
        backend,
    );
    sqlx::query(&sql)
        .bind(schedule_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_tick_runs_pending_schedules() {
        let cron_expr = "0 0 12 * * *";
        let schedule = Schedule::from_str(cron_expr).unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert!(next > Utc::now());
    }
}
