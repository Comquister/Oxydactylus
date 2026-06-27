CREATE TABLE users (
    id            TEXT        PRIMARY KEY,
    email         TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    is_admin      BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at    TEXT        NOT NULL
);

CREATE TABLE nodes (
    id         TEXT        PRIMARY KEY,
    name       TEXT        NOT NULL UNIQUE,
    grpc_addr  TEXT        NOT NULL,
    token      TEXT        NOT NULL,
    created_at TEXT        NOT NULL
);

CREATE TABLE eggs (
    id            TEXT        PRIMARY KEY,
    name          TEXT        NOT NULL,
    description   TEXT,
    author        TEXT,
    version       TEXT        NOT NULL DEFAULT '1.0.0',
    features      TEXT        NOT NULL DEFAULT '[]',
    file_denylist TEXT        NOT NULL DEFAULT '[]',
    docker_images TEXT        NOT NULL DEFAULT '{}',
    start_cmd     TEXT        NOT NULL,
    stop_cmd      TEXT        NOT NULL DEFAULT 'stop',
    startup_done  TEXT,
    created_at    TEXT        NOT NULL,
    updated_at    TEXT        NOT NULL
);

CREATE TABLE egg_variables (
    id            TEXT    PRIMARY KEY,
    egg_id        TEXT    NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    name          TEXT    NOT NULL,
    description   TEXT,
    env_variable  TEXT    NOT NULL,
    default_val   TEXT,
    user_viewable BOOLEAN NOT NULL DEFAULT TRUE,
    user_editable BOOLEAN NOT NULL DEFAULT TRUE,
    rules         TEXT,
    field_type    TEXT    NOT NULL DEFAULT 'text'
);

CREATE TABLE egg_install_scripts (
    id         TEXT NOT NULL PRIMARY KEY,
    egg_id     TEXT NOT NULL UNIQUE REFERENCES eggs(id) ON DELETE CASCADE,
    container  TEXT NOT NULL,
    entrypoint TEXT NOT NULL DEFAULT 'bash',
    script     TEXT NOT NULL
);

CREATE TABLE egg_config_files (
    id      TEXT PRIMARY KEY,
    egg_id  TEXT NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    path    TEXT NOT NULL,
    parser  TEXT NOT NULL,
    patches TEXT NOT NULL
);

CREATE TABLE allocations (
    id         TEXT    PRIMARY KEY,
    node_id    TEXT    NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    ip         TEXT    NOT NULL,
    ip_alias   TEXT,
    port       INTEGER NOT NULL,
    server_id  TEXT,
    created_at TEXT    NOT NULL,
    UNIQUE(node_id, ip, port)
);

CREATE TABLE servers (
    id               TEXT        PRIMARY KEY,
    user_id          TEXT        NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    node_id          TEXT        NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    egg_id           TEXT        REFERENCES eggs(id),
    allocation_id    TEXT        REFERENCES allocations(id),
    name             TEXT        NOT NULL UNIQUE,
    image            TEXT        NOT NULL,
    memory_mb        INT         NOT NULL,
    cpu_percent      INT         NOT NULL,
    env              TEXT        NOT NULL DEFAULT '[]',
    status           TEXT        NOT NULL DEFAULT 'stopped',
    database_limit   INTEGER     NOT NULL DEFAULT 0,
    backup_limit     INTEGER     NOT NULL DEFAULT 0,
    allocation_limit INTEGER     NOT NULL DEFAULT 1,
    created_at       TEXT        NOT NULL
);

CREATE TABLE server_subusers (
    id          TEXT        PRIMARY KEY,
    server_id   TEXT        NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id     TEXT        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permissions TEXT        NOT NULL DEFAULT '[]',
    created_at  TEXT        NOT NULL,
    UNIQUE (server_id, user_id)
);

ALTER TABLE nodes ADD COLUMN sftp_port INTEGER NOT NULL DEFAULT 2022;

CREATE TABLE database_hosts (
    id            TEXT    PRIMARY KEY,
    node_id       TEXT    REFERENCES nodes(id) ON DELETE SET NULL,
    name          TEXT    NOT NULL,
    host          TEXT    NOT NULL,
    port          INTEGER NOT NULL DEFAULT 3306,
    username      TEXT    NOT NULL,
    password      TEXT    NOT NULL,
    max_databases INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT    NOT NULL
);

CREATE TABLE server_databases (
    id            TEXT PRIMARY KEY,
    server_id     TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    host_id       TEXT NOT NULL REFERENCES database_hosts(id),
    database_name TEXT NOT NULL,
    username      TEXT NOT NULL,
    remote        TEXT NOT NULL DEFAULT '%',
    password      TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    UNIQUE(host_id, database_name)
);

CREATE TABLE schedules (
    id                TEXT    PRIMARY KEY,
    server_id         TEXT    NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name              TEXT    NOT NULL,
    cron_minute       TEXT    NOT NULL DEFAULT '*',
    cron_hour         TEXT    NOT NULL DEFAULT '*',
    cron_day_of_month TEXT    NOT NULL DEFAULT '*',
    cron_month        TEXT    NOT NULL DEFAULT '*',
    cron_day_of_week  TEXT    NOT NULL DEFAULT '*',
    is_active         BOOLEAN NOT NULL DEFAULT TRUE,
    is_processing     BOOLEAN NOT NULL DEFAULT FALSE,
    only_when_online  BOOLEAN NOT NULL DEFAULT FALSE,
    last_run_at       TEXT,
    next_run_at       TEXT,
    created_at        TEXT    NOT NULL
);

CREATE TABLE schedule_tasks (
    id                  TEXT    PRIMARY KEY,
    schedule_id         TEXT    NOT NULL REFERENCES schedules(id) ON DELETE CASCADE,
    sequence_id         INTEGER NOT NULL,
    action              TEXT    NOT NULL,
    payload             TEXT    NOT NULL,
    time_offset         INTEGER NOT NULL DEFAULT 0,
    is_queued           BOOLEAN NOT NULL DEFAULT FALSE,
    continue_on_failure BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TEXT    NOT NULL
);

CREATE TABLE backups (
    id           TEXT    PRIMARY KEY,
    server_id    TEXT    NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    uuid         TEXT    NOT NULL UNIQUE,
    name         TEXT    NOT NULL,
    ignored_files TEXT   NOT NULL DEFAULT '[]',
    driver       TEXT    NOT NULL DEFAULT 'local',
    sha256_hash  TEXT,
    bytes        INTEGER NOT NULL DEFAULT 0,
    is_successful BOOLEAN NOT NULL DEFAULT FALSE,
    is_locked    BOOLEAN NOT NULL DEFAULT FALSE,
    completed_at TEXT,
    created_at   TEXT    NOT NULL
);

CREATE TABLE activity_logs (
    id         TEXT PRIMARY KEY,
    batch_id   TEXT,
    server_id  TEXT REFERENCES servers(id) ON DELETE CASCADE,
    user_id    TEXT REFERENCES users(id) ON DELETE SET NULL,
    event      TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',
    ip         TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX activity_logs_server_id_idx ON activity_logs(server_id);
CREATE INDEX activity_logs_event_idx ON activity_logs(event);
