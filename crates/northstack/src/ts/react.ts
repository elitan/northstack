import { QueryClient, useMutation as useTanstackMutation, useSuspenseQuery } from "@tanstack/react-query";
import { call, type Input, type MutationName, type Output, type QueryName, RpcClientError } from "./client";

/** One cache for the whole app. */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 10_000,
      // Don't retry errors the server meant (validation, not found, auth).
      retry: (count, error) => count < 2 && (!(error instanceof RpcClientError) || error.code === "INTERNAL"),
    },
  },
});

/** Refetch every query on the page (after a change the page didn't make itself). */
export function refresh(): Promise<void> {
  return queryClient.invalidateQueries();
}

type QueryOptions<K extends QueryName> = {
  /** Poll: refetch every N ms (or decide from the current data; false stops). */
  refetchInterval?: number | false | ((data: Output<K> | undefined) => number | false);
};

/**
 * Helpers for one query procedure, generated into `api`:
 *
 * ```ts
 * const notes = api.notes.list.useQuery({ search });   // the data, never undefined
 * ```
 */
export function query<K extends QueryName>(name: K) {
  // `as string`: with no queries at all, K is `never` and has no methods.
  const path = (name as string).split(".");
  return {
    name,
    /**
     * Read data in a page or component. Suspends until it's loaded (the
     * page's loading.tsx shows meanwhile) and returns it, never undefined.
     * Errors go to the page's error.tsx; NOT_FOUND shows not-found.tsx.
     */
    useQuery: (input: Input<K>, options?: QueryOptions<K>): Output<K> => {
      const interval = options?.refetchInterval;
      return useSuspenseQuery<Output<K>, RpcClientError>({
        queryKey: [...path, input ?? null],
        queryFn: ({ signal }) => call(name, input, { signal }),
        refetchInterval: typeof interval === "function" ? (q) => interval(q.state.data) : interval,
      }).data;
    },
    /** Call the procedure directly, outside React. */
    call: (input: Input<K>) => call(name, input),
  };
}

/**
 * Helpers for one mutation procedure, generated into `api`:
 *
 * ```ts
 * const create = api.notes.create.useMutation();
 * create.mutate({ title });        // create.isPending, create.error, create.data
 * ```
 *
 * After a mutation succeeds, every query on the page refetches, so it shows
 * fresh data without listing what changed. `onSuccess` runs first: navigate
 * away there (e.g. after a delete) and the old page isn't refetched.
 */
export function mutation<K extends MutationName>(name: K) {
  return {
    name,
    useMutation: (options?: { onSuccess?: (data: Output<K>, input: Input<K>) => unknown }) =>
      useTanstackMutation<Output<K>, RpcClientError, Input<K>>({
        mutationFn: (input) => call(name, input),
        onSuccess: async (data, input) => {
          await options?.onSuccess?.(data, input);
          await queryClient.invalidateQueries();
        },
      }),
    /** Call it directly (e.g. from an event handler), then refetch queries. */
    call: async (input: Input<K>): Promise<Output<K>> => {
      const out = await call(name, input);
      await queryClient.invalidateQueries();
      return out;
    },
  };
}

/** True if the error is an RPC error with the given code, e.g. `isRpcError(e, "CONFLICT")`. */
export function isRpcError(error: unknown, code?: RpcClientError["code"]): error is RpcClientError {
  return error instanceof RpcClientError && (code === undefined || error.code === code);
}
