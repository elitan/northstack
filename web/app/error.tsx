// Any other error while a page renders or loads data. "Try again" refetches.
import type { ErrorProps } from "northstack";

export default function ErrorPage({ error, retry }: ErrorProps) {
  return (
    <div className="state">
      <h2>Something went wrong</h2>
      <p className="error">{error.message}</p>
      <button onClick={retry}>Try again</button>
    </div>
  );
}
