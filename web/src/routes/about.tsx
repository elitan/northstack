import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/about")({
  component: About,
});

function About() {
  return (
    <>
      <h1>About</h1>
      <p>
        A static route: no loader, no data. Add a file under <code>src/routes</code> and it becomes a page.
      </p>
    </>
  );
}
