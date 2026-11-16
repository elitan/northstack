-- The example feature. Comments inside CREATE TABLE become docs in Rust and
-- TypeScript: on the CREATE line for the table, above or after a column.
CREATE TABLE notes ( -- A note written by a user.
    id          UUID      PRIMARY KEY NOT NULL,
    title       TEXT      NOT NULL CHECK (length(title) BETWEEN 1 AND 200),
    body        TEXT      NOT NULL DEFAULT '',
    done        BOOLEAN   NOT NULL DEFAULT false,
    -- How important the note is. Sorted high to low in lists.
    priority    TEXT      NOT NULL DEFAULT 'normal' CHECK (priority IN ('low', 'normal', 'high')),
    -- Computed in the background after create/update; null until then.
    word_count  INTEGER,
    created_at  TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00')),
    updated_at  TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00'))
);

-- notes.list sorts by priority (high first), then newest; id breaks ties
-- between notes created in the same millisecond (v7 UUIDs grow over time).
-- An index with the same expressions lets SQLite read rows in order instead
-- of sorting the table.
CREATE INDEX notes_list ON notes (
    (CASE priority WHEN 'high' THEN 2 WHEN 'normal' THEN 1 ELSE 0 END) DESC,
    created_at DESC,
    id DESC
);

CREATE TABLE note_digests ( -- Daily statistics, written by the notes.daily_digest cron job.
    day         DATE      PRIMARY KEY NOT NULL,
    total       INTEGER   NOT NULL,
    done        INTEGER   NOT NULL,
    words       INTEGER   NOT NULL,
    created_at  TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00'))
);
