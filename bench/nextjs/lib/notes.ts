// The same procedures and SQL as server/src/features/notes in northstack (the Postgres version).
import { db, sql } from "./db";

const cols = ["id", "title", "body", "done", "word_count", "created_at", "updated_at", "priority"] as const;

type Out = { status: number; body: unknown };
const bad = (message: string): Out => ({ status: 400, body: { code: "BAD_REQUEST", message } });

function toNote(r: Record<string, unknown>) {
  return {
    id: r.id, title: r.title, body: r.body, done: r.done, wordCount: r.word_count,
    createdAt: r.created_at, updatedAt: r.updated_at, priority: r.priority,
  };
}

export async function call(name: string, input: any): Promise<Out> {
  switch (name) {
    case "notes.list": {
      const limit = input?.limit ?? 50;
      if (!(limit >= 1 && limit <= 200)) return bad("limit must be between 1 and 200");
      const search = typeof input?.search === "string" && input.search.trim() ? input.search.trim() : null;
      const done = typeof input?.done === "boolean" ? input.done : null;
      let q = db.selectFrom("notes").select(cols);
      if (search) q = q.where((eb) => eb.or([eb("title", "ilike", `%${search}%`), eb("body", "ilike", `%${search}%`)]));
      if (done !== null) q = q.where("done", "=", done);
      const rows = await q.orderBy("priority", "desc").orderBy("created_at", "desc").limit(limit).execute();
      return { status: 200, body: { notes: rows.map(toNote) } };
    }
    case "notes.get": {
      const row = await db.selectFrom("notes").select(cols).where("id", "=", input?.id).executeTakeFirst();
      return row ? { status: 200, body: toNote(row) } : { status: 404, body: { code: "NOT_FOUND", message: "note not found" } };
    }
    case "notes.create": {
      const title = String(input?.title ?? "").trim();
      if (!title) return bad("title must not be empty");
      if (title.length > 200) return bad("title must be at most 200 characters");
      // Note + background job row in one transaction, like the Rust app.
      const note = await db.transaction().execute(async (tx) => {
        const n = await tx.insertInto("notes")
          .values({ title, body: input?.body ?? "", priority: input?.priority ?? "normal" })
          .returning(cols).executeTakeFirstOrThrow();
        await tx.insertInto("northstack_jobs")
          .values({ kind: "notes.analyze", payload: JSON.stringify({ note_id: n.id }), max_attempts: 5, timeout_secs: 60 }).execute();
        return n;
      });
      return { status: 200, body: toNote(note) };
    }
    case "notes.stats": {
      const r = await sql<{ total: string; done: string; words: string; last_digest_day: string | null }>`
        SELECT count(*) AS total, count(*) FILTER (WHERE done) AS done,
               COALESCE(sum(word_count), 0)::bigint AS words,
               (SELECT max(day) FROM note_digests) AS last_digest_day
        FROM notes`.execute(db);
      const s = r.rows[0];
      return { status: 200, body: { total: Number(s.total), done: Number(s.done), words: Number(s.words), lastDigestDay: s.last_digest_day } };
    }
    default:
      return { status: 404, body: { code: "NOT_FOUND", message: `unknown procedure ${name}` } };
  }
}
