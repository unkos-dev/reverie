import { Observable, createOperationDescriptor, getRequest } from "relay-runtime";
import type {
  Environment,
  GraphQLResponse,
  GraphQLTaggedNode,
  OperationType,
  RequestParameters,
  Variables,
} from "relay-runtime";
import { z } from "zod";

import { invokeUnauthenticatedHandler } from "@/lib/query/client";
import { ContentRatingSchema, ReadingStatusSchema, ResourceEtagSchema } from "./books";
import type { mutationsAuthorPageQuery } from "@/lib/relay/__generated__/mutationsAuthorPageQuery.graphql";
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
      currentEtag: ResourceEtagSchema.optional(),
      lockedFields: z.array(z.string()).optional(),
      errors: z.record(z.string(), z.string()).optional(),
    })
    .optional(),
});
const envelope = z.object({
  data: z.record(z.string(), z.unknown()).nullable().optional(),
  errors: z.array(executionError).nonempty().optional(),
});

const uuid = z.uuid();
function domainId(kind: string) {
  return z.string().refine((value) => {
    try {
      const decoded = atob(value);
      return (
        btoa(decoded) === value &&
        decoded.startsWith(`${kind}:`) &&
        uuid.safeParse(decoded.slice(kind.length + 1)).success
      );
    } catch {
      return false;
    }
  }, `Invalid ${kind} ID.`);
}
const revision = z.object({
  workId: domainId("Work"),
  manifestationId: domainId("Manifestation"),
  etag: ResourceEtagSchema,
});
const titleSnapshot = revision.extend({ title: z.string() });
const formSnapshot = titleSnapshot.extend({
  subtitle: z.string().nullable(),
  description: z.string().nullable(),
  language: z.string().nullable(),
  publisher: z.string().nullable(),
  publicationDate: z.string().nullable(),
  isbn10: z.string().nullable(),
  isbn13: z.string().nullable(),
  isbnMismatch: z.boolean(),
  pages: z.number().int().nullable(),
  contentRating: ContentRatingSchema.nullable(),
});
const pageInfo = z.object({
  hasNextPage: z.boolean(),
  hasPreviousPage: z.boolean(),
  startCursor: z.string().nullable(),
  endCursor: z.string().nullable(),
});
const authorSnapshot = revision.extend({
  authors: z.object({
    edges: z.array(
      z.object({
        cursor: z.string(),
        node: z.object({ id: domainId("Author"), name: z.string() }),
      }),
    ),
    pageInfo,
  }),
});
const readingSnapshot = z.object({
  etag: ResourceEtagSchema,
  status: ReadingStatusSchema.nullable(),
  rating: z.number().int().nullable(),
  notes: z.string().nullable(),
  progressPct: z.number().nullable(),
  startedAt: z.iso.datetime().nullable(),
  finishedAt: z.iso.datetime().nullable(),
  lastReadAt: z.iso.datetime().nullable(),
});
const operations = z.array(
  z.object({
    operationId: uuid,
    fieldName: z.string(),
    expiresAt: z.iso.datetime(),
    status: z.enum(["eligible", "locked", "unavailable", "consumed"]),
  }),
);
const shelf = z.object({
  id: domainId("Shelf"),
  name: z.string(),
  isSystem: z.boolean(),
  itemCount: z.number().int(),
  createdAt: z.iso.datetime(),
  updatedAt: z.iso.datetime(),
});
const editingProjections = new Map<string, z.ZodType>();
for (const [name, snapshot] of [
  ["Title", titleSnapshot],
  ["Form", formSnapshot],
  ["Authors", authorSnapshot],
] as const) {
  editingProjections.set(
    `mutationsSave${name}Mutation`,
    z.object({
      updateMetadata: z.object({ requestId: uuid.nullable(), operations, metadata: snapshot }),
    }),
  );
  for (const [action, root, kind] of [
    ["Undo", "undoMetadata", "Mutation"],
    ["Resolve", "metadataOperation", "Query"],
  ] as const) {
    editingProjections.set(
      `mutations${action}${name}${kind}`,
      z.object({
        [root]: z.object({
          outcome: z.enum(["saved", "restored", "unknown"]),
          operations,
          metadata: snapshot,
        }),
      }),
    );
  }
}
for (const [name, snapshot] of [
  ["Title", titleSnapshot],
  ["Form", formSnapshot],
  ["Revision", revision],
  ["AuthorPage", authorSnapshot],
] as const) {
  editingProjections.set(
    `mutations${name}Query`,
    z.object({
      manifestation: z.object({ id: domainId("Manifestation"), metadata: snapshot }).nullable(),
    }),
  );
}
editingProjections.set(
  "readerAuthorEditingPaginationQuery",
  z.object({
    fetch__Manifestation: z
      .object({ id: domainId("Manifestation"), metadata: authorSnapshot })
      .nullable(),
  }),
);
editingProjections.set(
  "mutationsReadingQuery",
  z.object({
    manifestation: z
      .object({ id: domainId("Manifestation"), readingState: readingSnapshot })
      .nullable(),
  }),
);
editingProjections.set(
  "mutationsReadingMutation",
  z.object({ updateReadingState: readingSnapshot }),
);
for (const action of ["Create", "Rename"] as const)
  editingProjections.set(
    `mutations${action}ShelfMutation`,
    z.object({ [`${action.toLowerCase()}Shelf`]: shelf }),
  );
editingProjections.set(
  "mutationsDeleteShelfMutation",
  z.object({ deleteShelf: z.object({ shelfId: domainId("Shelf") }) }),
);
for (const action of ["Add", "Remove"] as const)
  editingProjections.set(
    `mutations${action}ShelfItemMutation`,
    z.object({
      [`${action.toLowerCase()}ShelfItem`]: z.object({
        shelfId: domainId("Shelf"),
        manifestationId: domainId("Manifestation"),
      }),
    }),
  );
for (const [action, root] of [
  ["AcceptMetadataVersion", "acceptMetadataVersion"],
  ["RevertMetadataField", "revertMetadataField"],
] as const)
  editingProjections.set(
    `mutations${action}Mutation`,
    z.object({
      [root]: z.object({
        manifestationId: domainId("Manifestation"),
        metadata: formSnapshot.nullable(),
      }),
    }),
  );
for (const [action, root] of [
  ["RejectMetadataVersion", "rejectMetadataVersion"],
  ["LockMetadataField", "lockMetadataField"],
  ["UnlockMetadataField", "unlockMetadataField"],
] as const)
  editingProjections.set(
    `mutations${action}Mutation`,
    z.object({ [root]: z.object({ manifestationId: domainId("Manifestation") }) }),
  );

/** Execute an editing document and capture its validated raw projection. */
export function executeEditing<T extends OperationType>(
  environment: Environment,
  document: GraphQLTaggedNode,
  variables: T["variables"],
): Observable<T["response"]> {
  const request = getRequest(document);
  if (!editingProjections.has(request.params.name))
    throw new Error("An editing document is required.");
  const operation = createOperationDescriptor(request, variables);
  const source =
    request.params.operationKind === "mutation"
      ? environment.executeMutation({ operation })
      : environment.execute({ operation });
  return source.map((payload) => {
    if (!("data" in payload) || payload.data == null)
      throw new Error("Missing editing projection.");
    return structuredClone(payload.data);
  });
}
type AuthorPage = {
  after: string | null;
  snapshot: NonNullable<mutationsAuthorPageQuery["response"]["manifestation"]>["metadata"];
};
/** Capture one complete author field from pages belonging to the same revision. */
export function captureAuthorPages(pages: readonly AuthorPage[]): {
  workId: string;
  manifestationId: string;
  etag: string;
  authors: readonly { id: string; name: string }[];
} {
  if (pages.length === 0) throw new Error("Complete author pages are required.");
  const first = pages[0];
  const base = authorSnapshot.parse(first.snapshot);
  const authors: { id: string; name: string }[] = [];
  let after: string | null = null;
  let hasNext = true;
  for (const page of pages) {
    const snapshot = authorSnapshot.parse(page.snapshot);
    if (
      !hasNext ||
      page.after !== after ||
      snapshot.etag !== base.etag ||
      snapshot.workId !== base.workId ||
      snapshot.manifestationId !== base.manifestationId ||
      (after === null && snapshot.authors.pageInfo.hasPreviousPage)
    )
      throw new Error("Author pages do not form one matched field.");
    authors.push(...snapshot.authors.edges.map((edge) => edge.node));
    after = snapshot.authors.pageInfo.endCursor;
    hasNext = snapshot.authors.pageInfo.hasNextPage;
    if (hasNext && after === null) throw new Error("An author continuation cursor is required.");
  }
  if (hasNext) throw new Error("Complete author pages are required.");
  return { workId: base.workId, manifestationId: base.manifestationId, etag: base.etag, authors };
}

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
        editingProjections.get(request.name)?.parse(result.data);
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
