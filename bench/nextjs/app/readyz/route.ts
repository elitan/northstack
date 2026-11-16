import { db, sql } from "@/lib/db";
export const dynamic = "force-dynamic";
export async function GET() {
  try { await sql`SELECT 1`.execute(db); return new Response("ready"); }
  catch { return new Response("not ready", { status: 503 }); }
}
