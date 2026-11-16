-- northstack: background job queue. Managed by the framework; do not edit.
-- Times are ISO 8601 text in UTC ('2026-11-15T10:00:00.123+00:00'), which sorts
-- chronologically as text.
CREATE TABLE northstack_jobs (
    id            INTEGER PRIMARY KEY,
    kind          TEXT    NOT NULL,
    payload       TEXT    NOT NULL,
    status        TEXT    NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued', 'running', 'done', 'dead')),
    attempts      INTEGER NOT NULL DEFAULT 0,
    max_attempts  INTEGER NOT NULL,
    timeout_secs  INTEGER NOT NULL,
    run_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00')),
    locked_until  TEXT,
    run_key       TEXT,
    last_error    TEXT,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00')),
    finished_at   TEXT
);

-- Idempotent enqueue (cron slots, "send once" jobs).
CREATE UNIQUE INDEX northstack_jobs_run_key ON northstack_jobs (run_key) WHERE run_key IS NOT NULL;
-- Fast claim of ready jobs and of jobs whose lease expired.
CREATE INDEX northstack_jobs_ready    ON northstack_jobs (run_at, id)   WHERE status = 'queued';
CREATE INDEX northstack_jobs_expired  ON northstack_jobs (locked_until) WHERE status = 'running';
CREATE INDEX northstack_jobs_finished ON northstack_jobs (finished_at)  WHERE status = 'done';
