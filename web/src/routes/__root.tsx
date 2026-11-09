import { createRootRouteWithContext, Link, Outlet } from "@tanstack/react-router";
import type { QueryClient } from "@tanstack/react-query";

/** Available in every route's `loader` and `beforeLoad` as `context`. */
export type RouterContext = {
  queryClient: QueryClient;
};

export const Route = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
});

function RootLayout() {
  return (
    <>
      <nav className="topbar">
        <Link to="/" className="brand">
          northstack
        </Link>
        <Link to="/" activeOptions={{ exact: true }} activeProps={{ className: "active" }}>
          Notes
        </Link>
        <Link to="/about" activeProps={{ className: "active" }}>
          About
        </Link>
      </nav>
      <main>
        <Outlet />
      </main>
    </>
  );
}
