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
CREATE TABLE notes (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    title       TEXT        NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    body        TEXT        NOT NULL DEFAULT '',
    done        BOOLEAN     NOT NULL DEFAULT false,
    -- Filled in asynchronously by the notes.analyze job.
    word_count  INT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX notes_created_at ON notes (created_at DESC);

-- One row per day, written by the daily digest cron job.
CREATE TABLE note_digests (
    day         DATE        PRIMARY KEY,
    total       BIGINT      NOT NULL,
    done        BIGINT      NOT NULL,
    words       BIGINT      NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TYPE note_priority AS ENUM ('low', 'normal', 'high');

ALTER TABLE notes ADD COLUMN priority note_priority NOT NULL DEFAULT 'normal';

COMMENT ON TABLE notes IS 'A note written by a user.';
COMMENT ON COLUMN notes.word_count IS 'Computed in the background after create/update; null until then.';
COMMENT ON COLUMN notes.priority IS 'How important the note is. Sorted high to low in lists.';
COMMENT ON TABLE note_digests IS 'Daily statistics, written by the notes.daily_digest cron job.';

-- Same as northstack's notes_list index: the list's ORDER BY reads rows in order.
CREATE INDEX notes_list ON notes (priority DESC, created_at DESC);
