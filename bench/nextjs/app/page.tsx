// The home page, rendered in the browser like northstack's: the server sends a
// static page (prerendered at build time) and the browser loads the notes from
// the API. This keeps the comparison apples to apples: "page" measures serving
// the page itself, and the data cost shows up in the notes.list tests.
"use client";

import { useEffect, useState } from "react";

export default function Page() {
  const [notes, setNotes] = useState<{ id: string; title: string }[]>([]);
  useEffect(() => {
    fetch("/rpc/notes.list", { method: "POST", headers: { "content-type": "application/json" }, body: '{"limit":20}' })
      .then((r) => r.json())
      .then((b) => setNotes(b.notes));
  }, []);
  return (
    <main>
      <h1>Notes</h1>
      <ul>{notes.map((n) => <li key={n.id}>{n.title}</li>)}</ul>
    </main>
  );
}
