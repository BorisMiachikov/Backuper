-- Initial schema for Backuper.
-- ID-поля хранятся как TEXT (UUID v7) для удобства совместимости с другими БД.

CREATE TABLE IF NOT EXISTS sources (
    id           TEXT PRIMARY KEY NOT NULL,
    kind         TEXT NOT NULL,
    name         TEXT NOT NULL,
    path         TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    description  TEXT,
    params_json  TEXT NOT NULL DEFAULT '{}',
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS source_tags (
    source_id TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    tag       TEXT NOT NULL,
    PRIMARY KEY (source_id, tag)
);

CREATE TABLE IF NOT EXISTS storages (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    config_json TEXT NOT NULL DEFAULT '{}',
    secret_ref  TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY NOT NULL,
    name            TEXT NOT NULL,
    source_id       TEXT NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    enabled         INTEGER NOT NULL DEFAULT 1,
    archive_cfg     TEXT NOT NULL DEFAULT '{}',
    retention_cfg   TEXT NOT NULL DEFAULT '{}',
    exclude_json    TEXT NOT NULL DEFAULT '{}',
    pre_cmd         TEXT,
    post_cmd        TEXT,
    priority        INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_jobs_source ON jobs(source_id);

CREATE TABLE IF NOT EXISTS schedules (
    id         TEXT PRIMARY KEY NOT NULL,
    job_id     TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    kind       TEXT NOT NULL,
    cron_expr  TEXT,
    run_at     TEXT,
    next_fire  TEXT,
    enabled    INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_schedules_next ON schedules(next_fire);

CREATE TABLE IF NOT EXISTS job_storages (
    job_id         TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    storage_id     TEXT NOT NULL REFERENCES storages(id) ON DELETE RESTRICT,
    order_idx      INTEGER NOT NULL DEFAULT 0,
    remote_path    TEXT NOT NULL DEFAULT '',
    overrides_json TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (job_id, storage_id)
);

CREATE TABLE IF NOT EXISTS job_runs (
    id            TEXT PRIMARY KEY NOT NULL,
    job_id        TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    trigger       TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    finished_at   TEXT,
    status        TEXT NOT NULL,
    bytes_in      INTEGER NOT NULL DEFAULT 0,
    bytes_out     INTEGER NOT NULL DEFAULT 0,
    files_count   INTEGER NOT NULL DEFAULT 0,
    archive_path  TEXT,
    error_msg     TEXT,
    host          TEXT NOT NULL DEFAULT '',
    attempt       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_runs_job_started ON job_runs(job_id, started_at DESC);

CREATE TABLE IF NOT EXISTS artifacts (
    id           TEXT PRIMARY KEY NOT NULL,
    run_id       TEXT NOT NULL REFERENCES job_runs(id) ON DELETE CASCADE,
    storage_id   TEXT NOT NULL REFERENCES storages(id) ON DELETE RESTRICT,
    remote_path  TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL DEFAULT 0,
    sha256       TEXT,
    uploaded_at  TEXT,
    verified_at  TEXT,
    status       TEXT NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_artifacts_run ON artifacts(run_id);

CREATE TABLE IF NOT EXISTS log_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id     TEXT REFERENCES job_runs(id) ON DELETE SET NULL,
    ts         TEXT NOT NULL,
    level      TEXT NOT NULL,
    component  TEXT NOT NULL,
    message    TEXT NOT NULL,
    kv_json    TEXT
);

CREATE INDEX IF NOT EXISTS idx_logs_run_ts ON log_events(run_id, ts);

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
