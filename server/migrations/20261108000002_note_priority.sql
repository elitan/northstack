CREATE TYPE note_priority AS ENUM ('low', 'normal', 'high');

ALTER TABLE notes ADD COLUMN priority note_priority NOT NULL DEFAULT 'normal';

COMMENT ON TABLE notes IS 'A note written by a user.';
COMMENT ON COLUMN notes.word_count IS 'Computed in the background after create/update; null until then.';
COMMENT ON COLUMN notes.priority IS 'How important the note is. Sorted high to low in lists.';
COMMENT ON TABLE note_digests IS 'Daily statistics, written by the notes.daily_digest cron job.';
