import type { Procedures } from "./procedures";
import type { RpcError } from "./types/RpcError";
import type { ErrorCode } from "./types/ErrorCode";

export type { Procedures };
export type ProcedureName = keyof Procedures;
export type QueryName = {
  [K in ProcedureName]: Procedures[K]["kind"] extends "query" ? K : never;
}[ProcedureName];
export type MutationName = {
  [K in ProcedureName]: Procedures[K]["kind"] extends "mutation" ? K : never;
}[ProcedureName];
export type Input<K extends ProcedureName> = Procedures[K]["input"];
export type Output<K extends ProcedureName> = Procedures[K]["output"];

/** Error thrown by `call` when a procedure fails. `code` mirrors the Rust `ErrorCode`. */
export class RpcClientError extends Error {
  readonly code: ErrorCode;
  readonly status: number;
  readonly procedure: string;

  constructor(procedure: string, status: number, error: RpcError) {
    super(error.message);
    this.name = "RpcClientError";
    this.procedure = procedure;
    this.status = status;
    this.code = error.code;
  }
}

export type ClientOptions = {
  /** Prefix for all requests. Defaults to "" (same origin). */
  baseUrl?: string;
  /** Extra headers for every request, e.g. authorization. */
  headers?: () => Record<string, string>;
};

let options: ClientOptions = {};

/** Configure the client once at startup. */
export function configureClient(next: ClientOptions): void {
  options = { ...options, ...next };
}

/** Call a procedure by name. Input and output are fully typed. */
export async function call<K extends ProcedureName>(
  name: K,
  input: Input<K>,
  init?: { signal?: AbortSignal },
): Promise<Output<K>> {
  const res = await fetch(`${options.baseUrl ?? ""}/rpc/${name}`, {
    method: "POST",
    headers: { "content-type": "application/json", ...(options.headers?.() ?? {}) },
    body: JSON.stringify(input ?? null),
    signal: init?.signal,
  });
  if (!res.ok) {
    let error: RpcError;
    try {
      error = (await res.json()) as RpcError;
    } catch {
      error = { code: "INTERNAL", message: `HTTP ${res.status}` };
    }
    throw new RpcClientError(name, res.status, error);
  }
  return (await res.json()) as Output<K>;
}
