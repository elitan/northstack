import { call } from "@/lib/notes";

export const dynamic = "force-dynamic";

export async function POST(req: Request, ctx: { params: Promise<{ name: string }> }) {
  const { name } = await ctx.params;
  let input: unknown = null;
  try { input = await req.json(); } catch { /* empty body */ }
  try {
    const out = await call(name, input);
    return Response.json(out.body, { status: out.status });
  } catch (e) {
    console.error(e);
    return Response.json({ code: "INTERNAL", message: "internal error" }, { status: 500 });
  }
}
