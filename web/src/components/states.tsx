import { Link, useRouter, type ErrorComponentProps } from "@tanstack/react-router";
import { useQueryErrorResetBoundary } from "@tanstack/react-query";
import { useEffect } from "react";

/** Shown while a route's loader is running (after a short delay). */
export function Pending() {
  return <p className="muted">Loading…</p>;
}

/** Shown for unknown URLs and when a loader throws `notFound()`. */
export function NotFound() {
  return (
    <div className="state">
      <h2>Not found</h2>
      <p className="muted">The page or item you're looking for doesn't exist.</p>
      <Link to="/">Back to notes</Link>
    </div>
  );
}

/** Shown when a loader or component throws. "Try again" refetches. */
export function ErrorView({ error }: ErrorComponentProps) {
  const router = useRouter();
  const { reset } = useQueryErrorResetBoundary();
  useEffect(() => reset(), [reset]);
  return (
    <div className="state">
      <h2>Something went wrong</h2>
      <p className="error">{error instanceof Error ? error.message : String(error)}</p>
      <button onClick={() => void router.invalidate()}>Try again</button>
    </div>
  );
}
