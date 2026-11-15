// Unknown URLs, notFound(), and NOT_FOUND errors from the API.
export default function NotFound() {
  return (
    <div className="state">
      <title>Not found · northstack</title>
      <h2>Not found</h2>
      <p className="muted">The page or item you're looking for doesn't exist.</p>
      <a href="/">Back to notes</a>
    </div>
  );
}
