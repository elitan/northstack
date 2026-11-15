// "/about": a static page, no data.
export default function About() {
  return (
    <>
      <title>About · northstack</title>
      <h1>About</h1>
      <p>
        A static page. Add a folder with a <code>page.tsx</code> under <code>web/app/</code> and it becomes a URL.
      </p>
    </>
  );
}
