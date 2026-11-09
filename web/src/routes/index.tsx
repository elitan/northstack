import { createFileRoute, Link } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { api } from "../rpc";
import type { ListNotesInput } from "../rpc/types/ListNotesInput";
import type { NotePriority } from "../rpc/types/NotePriority";
import { PrioritySelect } from "../components/priority";

/** URL search params for this page: /?q=milk&status=done */
type NotesSearch = {
  q?: string;
  status?: "open" | "done";
};

const toInput = (s: NotesSearch): ListNotesInput => ({
  search: s.q,
  done: s.status === undefined ? undefined : s.status === "done",
});

export const Route = createFileRoute("/")({
  // Parse and type the search params. Invalid values are dropped.
  validateSearch: (search: Record<string, unknown>): NotesSearch => ({
    q: typeof search.q === "string" && search.q !== "" ? search.q : undefined,
    status: search.status === "open" || search.status === "done" ? search.status : undefined,
  }),
  // Re-run the loader when these change.
  loaderDeps: ({ search }) => search,
  // Fetch before rendering: no loading spinners inside the page.
  loader: ({ context: { queryClient }, deps }) =>
    Promise.all([
      api.notes.list.ensureData(queryClient, toInput(deps)),
      api.notes.stats.ensureData(queryClient, null),
    ]),
  component: NotesPage,
});

function NotesPage() {
  const search = Route.useSearch();
  const { notes } = api.notes.list.useSuspenseQuery(toInput(search), {
    // Word counts are filled in by a background job; poll until they arrive.
    refetchInterval: (query) => (query.state.data?.notes.some((n) => n.wordCount === null) ? 1000 : false),
  });
  const stats = api.notes.stats.useSuspenseQuery(null);

  return (
    <>
      <header>
        <h1>Notes</h1>
        <p className="muted">
          {stats.total} notes · {stats.done} done · {stats.words} words
        </p>
      </header>

      <CreateNote />
      <Filters />

      <ul className="notes">
        {notes.map((note) => (
          <li key={note.id} className={note.done ? "done" : undefined}>
            <ToggleDone id={note.id} done={note.done} />
            <Link to="/notes/$noteId" params={{ noteId: note.id }} className="title">
              {note.title}
            </Link>
            {note.priority !== "normal" && <span className={`badge ${note.priority}`}>{note.priority}</span>}
            <span className="muted count">
              {note.wordCount === null ? "counting…" : `${note.wordCount} words`}
            </span>
          </li>
        ))}
      </ul>
      {notes.length === 0 && <p className="muted">No notes match.</p>}
    </>
  );
}

/** Filters live in the URL, so they survive reloads and can be shared. */
function Filters() {
  const search = Route.useSearch();
  const navigate = Route.useNavigate();
  // Keep typing snappy: local state, then update the URL after a pause.
  const [q, setQ] = useState(search.q ?? "");
  useEffect(() => {
    const id = setTimeout(() => {
      if (q !== (search.q ?? "")) {
        void navigate({ search: (prev) => ({ ...prev, q: q || undefined }), replace: true });
      }
    }, 250);
    return () => clearTimeout(id);
  }, [q, search.q, navigate]);

  return (
    <div className="toolbar">
      <input placeholder="Search…" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Search notes" />
      <select
        value={search.status ?? "all"}
        onChange={(e) => {
          const v = e.target.value;
          void navigate({
            search: (prev) => ({ ...prev, status: v === "open" || v === "done" ? v : undefined }),
          });
        }}
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
    invalidates: ["notes"],
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
  const update = api.notes.update.useMutation({ invalidates: ["notes"] });
  // Optimistic: show the requested state while the update is in flight.
  const checked = update.isPending && update.variables.done !== undefined ? update.variables.done : done;
  return (
    <input
      type="checkbox"
      checked={checked}
      onChange={(e) => update.mutate({ id, done: e.target.checked })}
      aria-label="Done"
    />
  );
}
