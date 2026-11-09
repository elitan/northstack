import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createRouter, RouterProvider } from "@tanstack/react-router";
import { routeTree } from "./routeTree.gen";
import { isRpcError } from "./rpc";
import { ErrorView, NotFound, Pending } from "./components/states";
import "./styles.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 10_000,
      // Don't retry errors the server meant (validation, not found, auth).
      retry: (count, error) => count < 2 && (!isRpcError(error) || error.code === "INTERNAL"),
    },
  },
});

const router = createRouter({
  routeTree,
  context: { queryClient },
  // Start loading a route's data when the user hovers or focuses a link.
  defaultPreload: "intent",
  // Freshness is handled by React Query, not the router cache.
  defaultPreloadStaleTime: 0,
  defaultPendingComponent: Pending,
  defaultErrorComponent: ErrorView,
  defaultNotFoundComponent: NotFound,
  scrollRestoration: true,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
);
