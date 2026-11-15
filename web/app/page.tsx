// "/": the notes list. Filters live in the URL (?q=milk&status=done), so they
// survive reloads and can be shared.
import { useEffect, useState } from "react";
import { api, navigate, type NotePriority, type PageProps } from "northstack";
import { PrioritySelect } from "./components/priority";

export default function NotesPage({ searchParams }: PageProps) {
  const status = searchParams.status;
  const { notes } = api.notes.list.useQuery(
    { search: searchParams.q, done: status === undefined ? undefined : status === "done" },
    // Word counts are filled in by a background job: poll until they arrive.
    { refetchInterval: (data) => (data?.notes.some((n) => n.wordCount === null) ? 1000 : false) },
  );
  const counting = notes.some((n) => n.wordCount === null);
  const stats = api.notes.stats.useQuery(null, { refetchInterval: counting ? 1000 : false });

  return (
    <>
      <title>Notes · northstack</title>
      <h1>Notes</h1>
      <p className="muted">
        {stats.total} notes · {stats.done} done · {stats.words} words
      </p>

      <CreateNote />
      <Filters q={searchParams.q ?? ""} status={status ?? "all"} />

      <ul className="notes">
        {notes.map((note) => (
          <li key={note.id} className={note.done ? "done" : undefined}>
            <ToggleDone id={note.id} done={note.done} />
            <a href={`/notes/${note.id}`} className="title">
              {note.title}
            </a>
            {note.priority !== "normal" && <span className={`badge ${note.priority}`}>{note.priority}</span>}
            <span className="muted count">{note.wordCount === null ? "counting…" : `${note.wordCount} words`}</span>
          </li>
        ))}
      </ul>
      {notes.length === 0 && <p className="muted">No notes match.</p>}
    </>
  );
}

/** Set one search param, keeping the others: "?q=milk" -> "?q=milk&status=done". */
function withParam(name: string, value: string | undefined): string {
  const params = new URLSearchParams(location.search);
  if (value) params.set(name, value);
  else params.delete(name);
  return `?${params}`;
}

function Filters({ q, status }: { q: string; status: string }) {
  // Keep typing snappy: local state, then update the URL after a pause.
  const [text, setText] = useState(q);
  useEffect(() => {
    if (text === q) return;
    const id = setTimeout(() => navigate(withParam("q", text.trim() || undefined), { replace: true }), 250);
    return () => clearTimeout(id);
  }, [text, q]);

  return (
    <div className="toolbar">
      <input placeholder="Search…" value={text} onChange={(e) => setText(e.target.value)} aria-label="Search notes" />
      <select
        value={status}
        onChange={(e) => navigate(withParam("status", e.target.value === "all" ? undefined : e.target.value))}
        aria-label="Filter"
      >
        <option value="all">All</option>
        <option value="open">Open</option>
        <option value="done">Done</option>
      </select>
    </div>
  );
}

function CreateNote() {
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [priority, setPriority] = useState<NotePriority>("normal");
  const create = api.notes.create.useMutation({
    onSuccess: () => {
      setTitle("");
      setBody("");
      setPriority("normal");
    },
  });

  return (
    <form
      className="create"
      onSubmit={(e) => {
        e.preventDefault();
        create.mutate({ title, body, priority });
      }}
    >
      <input placeholder="Title" value={title} onChange={(e) => setTitle(e.target.value)} aria-label="Title" />
      <textarea placeholder="Write something…" value={body} onChange={(e) => setBody(e.target.value)} aria-label="Body" />
      <div className="row">
        <PrioritySelect value={priority} onChange={setPriority} />
        <button type="submit" disabled={create.isPending}>
          {create.isPending ? "Saving…" : "Add note"}
        </button>
        {create.error && <span className="error">{create.error.message}</span>}
      </div>
    </form>
  );
}

function ToggleDone({ id, done }: { id: string; done: boolean }) {
  const update = api.notes.update.useMutation();
  // Optimistic: show the requested state while the update is in flight.
  const checked = update.isPending && update.variables?.done !== undefined ? update.variables.done : done;
  return (
    <input
      type="checkbox"
      checked={checked}
      onChange={(e) => update.mutate({ id, done: e.target.checked })}
      aria-label="Done"
    />
  );
}
