// "/notes/:id": one note. An unknown id (NOT_FOUND from the API) shows
// app/not-found.tsx automatically.
import { useState } from "react";
import { api, navigate, notFound, type Note, type PageProps } from "northstack";
import { PrioritySelect } from "../../components/priority";

export default function NotePage({ params }: PageProps<{ id: string }>) {
  if (!/^[0-9a-f-]{36}$/i.test(params.id)) notFound();
  const note = api.notes.get.useQuery({ id: params.id });
  return (
    <>
      <title>{`${note.title} · northstack`}</title>
      <a href="/" className="back">
        ← All notes
      </a>
      {/* key: reset the form when navigating to another note */}
      <EditNote key={note.id} note={note} />
    </>
  );
}

function EditNote({ note }: { note: Note }) {
  const [title, setTitle] = useState(note.title);
  const [body, setBody] = useState(note.body);
  const [priority, setPriority] = useState(note.priority);
  const dirty = title !== note.title || body !== note.body || priority !== note.priority;

  const save = api.notes.update.useMutation();
  // Leave first, so the deleted note isn't refetched.
  const remove = api.notes.delete.useMutation({ onSuccess: () => navigate("/") });

  return (
    <form
      className="create"
      onSubmit={(e) => {
        e.preventDefault();
        save.mutate({ id: note.id, title, body, priority });
      }}
    >
      <input value={title} onChange={(e) => setTitle(e.target.value)} aria-label="Title" />
      <textarea value={body} onChange={(e) => setBody(e.target.value)} aria-label="Body" rows={8} />
      <p className="muted">
        {note.wordCount === null ? "Counting words…" : `${note.wordCount} words`} · updated{" "}
        {new Date(note.updatedAt).toLocaleString()}
      </p>
      <div className="row">
        <PrioritySelect value={priority} onChange={setPriority} />
        <button type="submit" disabled={!dirty || save.isPending}>
          {save.isPending ? "Saving…" : "Save"}
        </button>
        <button type="button" className="danger" onClick={() => remove.mutate({ id: note.id })}>
          Delete
        </button>
        {save.isSuccess && !dirty && <span className="muted">Saved</span>}
        {(save.error ?? remove.error) && <span className="error">{(save.error ?? remove.error)?.message}</span>}
      </div>
    </form>
  );
}
