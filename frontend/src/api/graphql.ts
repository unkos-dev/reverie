import { Observable } from "relay-runtime";
import type { GraphQLResponse, RequestParameters, Variables } from "relay-runtime";
import { z } from "zod";

import { invokeUnauthenticatedHandler } from "@/lib/query/client";
import { ApiError } from "./errors";
import { apiFetch } from "./fetch";

const executionError = z.object({
  message: z.string().min(1),
  locations: z
    .array(z.object({ line: z.number().int().positive(), column: z.number().int().positive() }))
    .optional(),
  path: z.array(z.union([z.string(), z.number().int()])).optional(),
  extensions: z
    .object({
      code: z.string().optional(),
      errors: z.record(z.string(), z.string()).optional(),
    })
    .optional(),
});
const envelope = z.object({
  data: z.record(z.string(), z.unknown()).nullable().optional(),
  errors: z.array(executionError).nonempty().optional(),
});

export class GraphQLExecutionError extends Error {
  readonly errors: z.infer<typeof executionError>[];

  constructor(errors: z.infer<typeof executionError>[]) {
    super(errors.map((error) => error.message).join("; "));
    this.name = "GraphQLExecutionError";
    this.errors = errors;
  }
}

export function graphqlNetwork(
  request: RequestParameters,
  variables: Variables,
  accountSignal: AbortSignal,
): Observable<GraphQLResponse> {
  return Observable.create((sink) => {
    const controller = new AbortController();
    const signal = AbortSignal.any([controller.signal, accountSignal]);
    if (request.id === null) {
      sink.error(new Error("A generated document ID is required."));
      return;
    }
    void (async () => {
      try {
        const raw = await apiFetch("/api/graphql", {
          method: "POST",
          body: JSON.stringify({ id: request.id, variables }),
          signal,
        });
        const result = envelope.parse(raw);
        if (signal.aborted) return;
        if (result.errors !== undefined) throw new GraphQLExecutionError(result.errors);
        if (result.data === undefined || result.data === null)
          throw new Error("Missing GraphQL response data.");
        sink.next({ data: result.data });
        sink.complete();
      } catch (error) {
        if (signal.aborted) return;
        if (error instanceof ApiError && error.status === 401) invokeUnauthenticatedHandler();
        sink.error(error instanceof Error ? error : new Error("GraphQL request failed."));
      }
    })();
    return () => {
      controller.abort();
    };
  });
}
