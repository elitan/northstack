// The entry point (loaded by web/index.html): the router inside one React
// Query cache. Pages live in web/app/.
import { QueryClientProvider } from "@tanstack/react-query";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { queryClient } from "./react";
import { Router, interceptLinks } from "./router";

interceptLinks();
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Router />
    </QueryClientProvider>
  </StrictMode>,
);
