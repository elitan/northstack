import {
  queryOptions as tanstackQueryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  useSuspenseQuery,
  type QueryClient,
  type QueryKey,
  type UseMutationOptions,
  type UseQueryOptions,
  type UseSuspenseQueryOptions,
} from "@tanstack/react-query";
import { call, type Input, type MutationName, type Output, type QueryName, RpcClientError } from "./client";
import type { Namespace } from "./procedures";

/** Something to invalidate: a whole namespace ("notes") or one query ("notes.list"). */
export type InvalidateTarget = Namespace | QueryName;

type QueryOpts<K extends QueryName> = Omit<
  UseQueryOptions<Output<K>, RpcClientError, Output<K>, QueryKey>,
  "queryKey" | "queryFn"
>;

type SuspenseOpts<K extends QueryName> = Omit<
  UseSuspenseQueryOptions<Output<K>, RpcClientError, Output<K>, QueryKey>,
  "queryKey" | "queryFn"
>;

/**
 * Typed helpers for one query procedure. Generated into `api.ts`:
 *
 * ```ts
 * api.notes.list.useSuspenseQuery({ search })            // in components
 * queryClient.ensureQueryData(api.notes.list.queryOptions({ search }))  // in route loaders
 * api.notes.list.key()                                    // ["notes", "list"] for invalidation
 * ```
 */
export function query<K extends QueryName>(name: K) {
  const path = name.split(".");
  const queryOptions = (input: Input<K>, options?: QueryOpts<K>) =>
    tanstackQueryOptions<Output<K>, RpcClientError, Output<K>, QueryKey>({
      queryKey: [...path, input ?? null],
      queryFn: ({ signal }) => call(name, input, { signal }),
      ...options,
    });
  return {
    name,
    /** Query key. Without input it matches every input (for invalidation). */
    key: (input?: Input<K>): QueryKey => (input === undefined ? path : [...path, input ?? null]),
    /** Call the procedure directly, outside React Query. */
    call: (input: Input<K>, init?: { signal?: AbortSignal }) => call(name, input, init),
    /** Options for `useQuery`, `queryClient.ensureQueryData`, `prefetchQuery`, ... */
    queryOptions,
    /** Fetch in a route loader so the component never shows a spinner. */
    ensureData: (queryClient: QueryClient, input: Input<K>) => queryClient.ensureQueryData(queryOptions(input)),
    useQuery: (input: Input<K>, options?: QueryOpts<K>) => useQuery(queryOptions(input, options)),
    /** Preferred in route components: returns the data, never undefined. */
    useSuspenseQuery: (input: Input<K>, options?: SuspenseOpts<K>) =>
      useSuspenseQuery<Output<K>, RpcClientError, Output<K>, QueryKey>({
        queryKey: [...path, input ?? null],
        queryFn: ({ signal }) => call(name, input, { signal }),
        ...options,
      }).data,
  };
}

type MutationOpts<K extends MutationName> = Omit<
  UseMutationOptions<Output<K>, RpcClientError, Input<K>>,
  "mutationFn"
> & {
  /** Queries to refetch after success, e.g. `["notes"]` or `["notes.list", "notes.stats"]`. */
  invalidates?: InvalidateTarget[];
};

/**
 * Typed helpers for one mutation procedure. Generated into `api.ts`:
 *
 * ```ts
 * const create = api.notes.create.useMutation({ invalidates: ["notes"] });
 * create.mutate({ title });
 * ```
 */
export function mutation<K extends MutationName>(name: K) {
  return {
    name,
    call: (input: Input<K>) => call(name, input),
    useMutation: (options?: MutationOpts<K>) => {
      const queryClient = useQueryClient();
      const { invalidates, onSuccess, ...rest } = options ?? {};
      return useMutation<Output<K>, RpcClientError, Input<K>>({
        mutationFn: (input) => call(name, input),
        ...rest,
        onSuccess: async (...args) => {
          await invalidate(queryClient, ...(invalidates ?? []));
          return onSuccess?.(...args);
        },
      });
    },
  };
}

/** Refetch everything under the given namespaces / queries. */
export function invalidate(queryClient: QueryClient, ...targets: InvalidateTarget[]) {
  return Promise.all(targets.map((t) => queryClient.invalidateQueries({ queryKey: t.split(".") })));
}

/** Hook form of `invalidate`. */
export function useInvalidate() {
  const queryClient = useQueryClient();
  return (...targets: InvalidateTarget[]) => invalidate(queryClient, ...targets);
}

/** True if the error is an RPC error with the given code, e.g. `isRpcError(e, "NOT_FOUND")`. */
export function isRpcError(error: unknown, code?: RpcClientError["code"]): error is RpcClientError {
  return error instanceof RpcClientError && (code === undefined || error.code === code);
}
