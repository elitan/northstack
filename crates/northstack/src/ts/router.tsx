// The folder router. Files in web/app/ become URLs, like Next.js:
//
//   app/page.tsx                 /
//   app/about/page.tsx           /about
//   app/notes/[id]/page.tsx      /notes/:id      (params.id)
//   app/(admin)/users/page.tsx   /users          ((group) folders don't appear in the URL)
//   app/layout.tsx               wraps every page below its folder
//   app/not-found.tsx            unknown URLs, notFound(), and NOT_FOUND from the API
//   app/error.tsx                any other error while rendering (gets { error, retry })
//   app/loading.tsx              shown while a page's data loads
//
// Plain <a href="/notes/1"> links navigate without a full page load.

import { QueryErrorResetBoundary } from "@tanstack/react-query";
import React, { Component, Suspense, createContext, lazy, useContext, useSyncExternalStore } from "react";
import { RpcClientError } from "./client";

type Comp = React.ComponentType<any>;
type Lazy = () => Promise<{ default: Comp }>;

/** Props every page receives. */
export type PageProps<Params extends Record<string, string> = Record<string, string>> = {
  params: Params;
  searchParams: Record<string, string | undefined>;
};

/** Props every layout receives. */
export type LayoutProps = { children: React.ReactNode; params: Record<string, string> };

/** Props of app/error.tsx. */
export type ErrorProps = { error: Error; retry: () => void };

const pageFiles = import.meta.glob("/app/**/page.tsx") as Record<string, Lazy>;
const layoutFiles = import.meta.glob("/app/**/layout.tsx") as Record<string, Lazy>;
const special = import.meta.glob(["/app/not-found.tsx", "/app/error.tsx", "/app/loading.tsx"], {
  eager: true,
}) as Record<string, { default: Comp }>;

// Each file becomes one React.lazy component, created once.
const lazyCache = new Map<Lazy, Comp>();
const load = (m: Lazy) => lazyCache.get(m) ?? (lazyCache.set(m, lazy(m)), lazyCache.get(m)!);

type Route = { pattern: string[]; page: Lazy; layouts: Lazy[] };

const isGroup = (dir: string) => /^\(.*\)$/.test(dir);

function layoutsFor(dirs: string[]): Lazy[] {
  const chain: Lazy[] = [];
  for (let i = 0; i <= dirs.length; i++) {
    const layout = layoutFiles[["/app", ...dirs.slice(0, i), "layout.tsx"].join("/")];
    if (layout) chain.push(layout);
  }
  return chain;
}

const routes: Route[] = Object.entries(pageFiles)
  .map(([file, page]) => {
    const dirs = file.replace(/^\/app\/?/, "").split("/").slice(0, -1);
    return { pattern: dirs.filter((d) => !isGroup(d)), page, layouts: layoutsFor(dirs) };
  })
  // Static segments win over parameters: /notes/new before /notes/[id].
  .sort((a, b) => {
    const score = (r: Route) => r.pattern.map((s) => (s.startsWith("[") ? "1" : "0")).join("");
    return score(a).localeCompare(score(b));
  });

function match(pathname: string): { route: Route; params: Record<string, string> } | null {
  const parts = pathname.split("/").filter(Boolean).map(decodeURIComponent);
  for (const route of routes) {
    if (route.pattern.length !== parts.length) continue;
    const params: Record<string, string> = {};
    const ok = route.pattern.every((seg, i) => {
      if (seg.startsWith("[")) {
        params[seg.slice(1, -1)] = parts[i]!; // same length as the pattern
        return true;
      }
      return seg === parts[i];
    });
    if (ok) return { route, params };
  }
  return null;
}

// --- location ----------------------------------------------------------------

const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  addEventListener("popstate", l);
  return () => {
    listeners.delete(l);
    removeEventListener("popstate", l);
  };
};
const currentUrl = () => location.pathname + location.search;

/** Go to another page without a full reload. `navigate("?q=milk")` changes only the search. */
export function navigate(href: string, options?: { replace?: boolean }): void {
  const url = new URL(href, location.href);
  if (url.origin !== location.origin) {
    location.href = url.href;
    return;
  }
  history[options?.replace ? "replaceState" : "pushState"](null, "", url);
  notify();
  if (!options?.replace && !url.hash) scrollTo(0, 0);
}

/** The current path, e.g. "/notes/1". */
export function usePathname(): string {
  return new URL(useSyncExternalStore(subscribe, currentUrl), location.origin).pathname;
}

/** The current search params (?q=milk). Change them with navigate("?q=..."). */
export function useSearchParams(): URLSearchParams {
  return new URLSearchParams(useSyncExternalStore(subscribe, currentUrl).split("?")[1] ?? "");
}

const ParamsContext = createContext<Record<string, string>>({});

/** The current route's params, e.g. { id: "1" } for app/notes/[id]/page.tsx. */
export function useParams<P extends Record<string, string> = Record<string, string>>(): P {
  return useContext(ParamsContext) as P;
}

// --- errors --------------------------------------------------------------------

class NotFoundError extends Error {}

/** Show app/not-found.tsx instead of the current page. */
export function notFound(): never {
  throw new NotFoundError("not found");
}

const isNotFound = (e: unknown) =>
  e instanceof NotFoundError || (e instanceof RpcClientError && e.code === "NOT_FOUND");

const DefaultNotFound = () => <h1>Not found</h1>;
const DefaultError = ({ error, retry }: ErrorProps) => (
  <div>
    <h1>Something went wrong</h1>
    <p>{error.message}</p>
    <button onClick={retry}>Try again</button>
  </div>
);
const NotFoundPage = special["/app/not-found.tsx"]?.default ?? DefaultNotFound;
const ErrorPage = special["/app/error.tsx"]?.default ?? DefaultError;
const LoadingPage = special["/app/loading.tsx"]?.default ?? (() => null);

class Boundary extends Component<{ children: React.ReactNode; reset: () => void }, { error: unknown }> {
  state = { error: null as unknown };
  static getDerivedStateFromError(error: unknown) {
    return { error };
  }
  render() {
    const { error } = this.state;
    if (error === null) return this.props.children;
    if (isNotFound(error)) return <NotFoundPage />;
    const retry = () => {
      this.props.reset();
      this.setState({ error: null });
    };
    return <ErrorPage error={error instanceof Error ? error : new Error(String(error))} retry={retry} />;
  }
}

// --- rendering -------------------------------------------------------------------

function Router() {
  const url = useSyncExternalStore(subscribe, currentUrl);
  const { pathname, search } = new URL(url, location.origin);
  const m = match(pathname);
  const params = m?.params ?? {};
  const searchParams = Object.fromEntries(new URLSearchParams(search));

  const Page = m ? load(m.route.page) : NotFoundPage;
  // The page, with loading and error states. (A const: the render function
  // below runs later, after `node` has been wrapped in layouts.)
  const page = (
    <QueryErrorResetBoundary>
      {({ reset }) => (
        <Boundary key={pathname} reset={reset}>
          <Suspense fallback={<LoadingPage />}>
            <Page params={params} searchParams={searchParams} />
          </Suspense>
        </Boundary>
      )}
    </QueryErrorResetBoundary>
  );
  let node: React.ReactNode = page;
  for (const layout of [...(m?.route.layouts ?? layoutsFor([]))].reverse()) {
    const Layout = load(layout);
    node = <Layout params={params}>{node}</Layout>;
  }
  return (
    <ParamsContext.Provider value={params}>
      <Suspense fallback={null}>{node}</Suspense>
    </ParamsContext.Provider>
  );
}

/** Clicks on same-origin <a href> navigate in the app instead of reloading. */
function interceptLinks() {
  document.addEventListener("click", (e) => {
    const a = (e.target as Element).closest?.("a");
    if (!a || !a.href || a.target || a.hasAttribute("download") || a.origin !== location.origin) return;
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    navigate(a.href);
  });
}

export { Router, interceptLinks };
