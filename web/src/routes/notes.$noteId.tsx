import { createFileRoute, Link, notFound, useNavigate } from "@tanstack/react-router";
import { useState } from "react";
import { api, isRpcError } from "../rpc";
import type { Note } from "../rpc/types/Note";
import { PrioritySelect } from "../components/priority";

export const Route = createFileRoute("/notes/$noteId")({
  loader: async ({ context: { queryClient }, params }) => {
    try {
      await api.notes.get.ensureData(queryClient, { id: params.noteId });
    } catch (error) {
      // Unknown id or not a UUID: show the not-found page.
      if (isRpcError(error, "NOT_FOUND") || isRpcError(error, "BAD_REQUEST")) throw notFound();
      throw error;
    }
  },
  component: NotePage,
});

function NotePage() {
  const { noteId } = Route.useParams();
  const note = api.notes.get.useSuspenseQuery({ id: noteId });
  return (
    <>
      <Link to="/" className="back">
        ← All notes
      </Link>
      {/* key: reset the form when navigating to another note */}
      <EditNote key={note.id} note={note} />
    </>
  );
}

function EditNote({ note }: { note: Note }) {
  const navigate = useNavigate();
  const [title, setTitle] = useState(note.title);
  const [body, setBody] = useState(note.body);
  const [priority, setPriority] = useState(note.priority);
  const dirty = title !== note.title || body !== note.body || priority !== note.priority;

  const save = api.notes.update.useMutation({ invalidates: ["notes"] });
  const remove = api.notes.delete.useMutation({
    // Not "notes": that would refetch this (deleted) note while leaving.
    invalidates: ["notes.list", "notes.stats"],
    onSuccess: () => navigate({ to: "/" }),
  });

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
