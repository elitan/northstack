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
