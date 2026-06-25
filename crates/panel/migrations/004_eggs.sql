CREATE TABLE eggs (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT        NOT NULL,
    description   TEXT,
    author        TEXT,
    version       TEXT        NOT NULL DEFAULT '1.0.0',
    features      TEXT[]      NOT NULL DEFAULT '{}',
    file_denylist TEXT[]      NOT NULL DEFAULT '{}',
    docker_images JSONB       NOT NULL DEFAULT '{}',
    start_cmd     TEXT        NOT NULL,
    stop_cmd      TEXT        NOT NULL DEFAULT 'stop',
    startup_done  TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE egg_variables (
    id            UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id        UUID    NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
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
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id     UUID NOT NULL UNIQUE REFERENCES eggs(id) ON DELETE CASCADE,
    container  TEXT NOT NULL,
    entrypoint TEXT NOT NULL DEFAULT 'bash',
    script     TEXT NOT NULL
);

CREATE TABLE egg_config_files (
    id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    egg_id  UUID NOT NULL REFERENCES eggs(id) ON DELETE CASCADE,
    path    TEXT NOT NULL,
    parser  TEXT NOT NULL CHECK (parser IN ('properties','json','yaml','ini','xml')),
    patches JSONB NOT NULL
);

ALTER TABLE servers
    ADD COLUMN owner_id  UUID  REFERENCES users(id),
    ADD COLUMN egg_id    UUID  REFERENCES eggs(id),
    ADD COLUMN status    TEXT  NOT NULL DEFAULT 'stopped'
                               CHECK (status IN ('installing','running','stopped','error')),
    ADD COLUMN env_vars  JSONB NOT NULL DEFAULT '{}';
