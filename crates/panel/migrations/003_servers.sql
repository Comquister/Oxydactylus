CREATE TABLE servers (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id     UUID        NOT NULL REFERENCES nodes(id) ON DELETE RESTRICT,
    name        TEXT        NOT NULL UNIQUE,
    image       TEXT        NOT NULL,
    memory_mb   INT         NOT NULL CHECK (memory_mb > 0),
    cpu_percent INT         NOT NULL CHECK (cpu_percent > 0),
    env         TEXT[]      NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
