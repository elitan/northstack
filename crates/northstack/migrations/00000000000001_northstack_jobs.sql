-- northstack: background job queue. Managed by the framework; do not edit.
CREATE TABLE northstack_jobs (
    id            BIGSERIAL PRIMARY KEY,
    kind          TEXT        NOT NULL,
    payload       JSONB       NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'queued'
                  CHECK (status IN ('queued', 'running', 'done', 'dead')),
    attempts      INT         NOT NULL DEFAULT 0,
    max_attempts  INT         NOT NULL,
    timeout_secs  INT         NOT NULL,
    run_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    locked_until  TIMESTAMPTZ,
    run_key       TEXT,
    last_error    TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at   TIMESTAMPTZ
);

-- Idempotent enqueue (cron slots, "send once" jobs).
CREATE UNIQUE INDEX northstack_jobs_run_key ON northstack_jobs (run_key) WHERE run_key IS NOT NULL;
-- Fast claim of ready jobs and of jobs whose lease expired.
CREATE INDEX northstack_jobs_ready   ON northstack_jobs (run_at, id)  WHERE status = 'queued';
CREATE INDEX northstack_jobs_expired ON northstack_jobs (locked_until) WHERE status = 'running';
CREATE INDEX northstack_jobs_finished ON northstack_jobs (finished_at) WHERE status = 'done';
