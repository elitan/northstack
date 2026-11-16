// Same schema as the Rust app (created by its migrations). Pool size matches
// the Rust default (DB_MAX_CONNECTIONS=10).
import { Kysely, PostgresDialect, sql, type Generated } from "kysely";
import { Pool } from "pg";

interface NotesTable {
  id: Generated<string>;
  title: string;
  body: Generated<string>;
  done: Generated<boolean>;
  word_count: number | null;
  created_at: Generated<Date>;
  updated_at: Generated<Date>;
  priority: Generated<"low" | "normal" | "high">;
}
interface JobsTable {
  kind: string;
  payload: unknown;
  max_attempts: number;
  timeout_secs: number;
}
interface DB {
  notes: NotesTable;
  northstack_jobs: JobsTable;
}

const g = globalThis as unknown as { db?: Kysely<DB> };
export const db =
  g.db ??
  (g.db = new Kysely<DB>({
    dialect: new PostgresDialect({
      pool: new Pool({
        connectionString: process.env.DATABASE_URL,
        max: Number(process.env.DB_MAX_CONNECTIONS ?? 10),
      }),
    }),
  }));
export { sql };
