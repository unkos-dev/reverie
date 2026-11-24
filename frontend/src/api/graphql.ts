import {
  Observable,
  type GraphQLResponse,
  type RequestParameters,
  type Variables,
} from "relay-runtime";
import { z } from "zod";

import { invokeUnauthenticatedHandler } from "@/lib/query/client";
import { ResourceEtagSchema } from "./books";
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
const ID_PREFIXES = {
  Author: "author",
  Genre: "genre",
  Manifestation: "book",
  MetadataRevision: "metadata_rev",
  MetadataVersion: "metadata_version",
  Mood: "mood",
  Series: "series",
  Shelf: "shelf",
  Tag: "tag",
  User: "user",
  Work: "work",
} as const;
type IdKind = keyof typeof ID_PREFIXES;
const CROCKFORD = "0123456789abcdefghjkmnpqrstvwxyz";
const TYPE_ID_SUFFIX = /^[0-7][0-9a-hjkmnp-tv-z]{25}$/;

function typeIdUuid(kind: IdKind, value: string): string | undefined {
  const prefix = `${ID_PREFIXES[kind]}_`;
  const suffix = value.slice(prefix.length);
  if (!value.startsWith(prefix) || !TYPE_ID_SUFFIX.test(suffix)) return undefined;
  let bits = 0n;
  for (const char of suffix) bits = (bits << 5n) | BigInt(CROCKFORD.indexOf(char));
  const hex = bits.toString(16).padStart(32, "0");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
export function domainId(kind: IdKind): z.ZodType<string> {
  return z.string().refine((value) => typeIdUuid(kind, value) !== undefined, `Invalid ${kind} ID.`);
}
function connection(node: z.ZodType) {
  return z.object({ edges: z.array(z.object({ node })) });
}
const authorIds = connection(z.object({ id: domainId("Author") }));
const workProjection = z.object({ id: domainId("Work"), authors: authorIds });
const shelfProjection = z.object({
  id: domainId("Shelf"),
  createdAt: z.iso.datetime(),
  updatedAt: z.iso.datetime(),
});
const shelfItems = z.object({
  id: domainId("Shelf"),
  items: connection(z.object({ id: domainId("Manifestation") })),
});
const seriesWorks = z.object({
  id: domainId("Series"),
  works: connection(
    workProjection.extend({
      manifestations: connection(z.object({ id: domainId("Manifestation") })),
    }),
  ),
});
const versionProjection = z.object({
  id: domainId("Manifestation"),
  versions: z.object({ pending: connection(z.object({ id: domainId("MetadataVersion") })) }),
});
const readerProjections = new Map<string, z.ZodType>([
  [
    "readerDetailQuery",
    z.object({
      manifestation: z
        .object({
          id: domainId("Manifestation"),
          createdAt: z.iso.datetime(),
          work: workProjection
            .extend({
              selectedSeries: z.object({ node: z.object({ id: domainId("Series") }) }).nullable(),
            })
            .nullable(),
          tags: connection(z.object({ id: domainId("Tag") })),
        })
        .nullable(),
    }),
  ],
  ["readerWorkAuthorsPaginationQuery", z.object({ fetch__Work: workProjection.nullable() })],
  [
    "readerDetailTagsPaginationQuery",
    z.object({
      fetch__Manifestation: z
        .object({
          id: domainId("Manifestation"),
          tags: connection(z.object({ id: domainId("Tag") })),
        })
        .nullable(),
    }),
  ],
  ["readerShelfQuery", z.object({ shelf: shelfProjection.extend(shelfItems.shape).nullable() })],
  ["readerShelfItemsPaginationQuery", z.object({ fetch__Shelf: shelfItems.nullable() })],
  ["readerSeriesQuery", z.object({ series: seriesWorks.nullable() })],
  ["readerSeriesWorksPaginationQuery", z.object({ fetch__Series: seriesWorks.nullable() })],
  [
    "CommandPaletteQuery",
    z.object({
      search: z.array(
        z.object({
          manifestation: z
            .object({
              id: domainId("Manifestation"),
              work: workProjection.nullable(),
            })
            .nullable(),
        }),
      ),
    }),
  ],
  ["readerVersionsQuery", z.object({ manifestation: versionProjection.nullable() })],
  [
    "readerVersionsPaginationQuery",
    z.object({ fetch__Manifestation: versionProjection.nullable() }),
  ],
]);
const shelf = z.object({
  id: domainId("Shelf"),
  name: z.string(),
  isSystem: z.boolean(),
  itemCount: z.number().int(),
  createdAt: z.iso.datetime(),
  updatedAt: z.iso.datetime(),
});
const refusal = <T extends string>(typename: T) =>
  z.object({ __typename: z.literal(typename), message: z.string() });
const editingProjections = new Map<string, z.ZodType>();

export function encodeGraphQLId(kind: IdKind, value: string): string {
  let bits = BigInt(`0x${uuid.parse(value).replaceAll("-", "")}`);
  let suffix = "";
  for (let index = 0; index < 26; index += 1) {
    suffix = CROCKFORD.charAt(Number(bits & 31n)) + suffix;
    bits >>= 5n;
  }
  return `${ID_PREFIXES[kind]}_${suffix}`;
}

/** The Book route for a manifestation UUID from a retained HTTP response. */
export function bookPath(manifestationId: string): string {
  return `/b/${encodeGraphQLId("Manifestation", manifestationId)}`;
}

export function decodeGraphQLId(kind: IdKind, value: string): string {
  const id = typeIdUuid(kind, value);
  if (id === undefined) throw new TypeError(`Invalid ${kind} ID.`);
  return id;
}
editingProjections.set(
  "mutationsCreateShelfMutation",
  z.object({
    createShelf: z.discriminatedUnion("__typename", [
      z.object({
        __typename: z.literal("ShelfCreated"),
        edge: z.object({ cursor: z.string(), node: shelf }),
      }),
      refusal("ShelfNameInvalid"),
    ]),
  }),
);
editingProjections.set(
  "mutationsRenameShelfMutation",
  z.object({
    renameShelf: z.discriminatedUnion("__typename", [
      z.object({
        __typename: z.literal("ShelfRenamed"),
        shelf: shelf.pick({ id: true, name: true, updatedAt: true }),
      }),
      refusal("ShelfNameInvalid"),
      refusal("SystemShelfImmutable"),
      refusal("NotFound"),
    ]),
  }),
);
editingProjections.set(
  "mutationsDeleteShelfMutation",
  z.object({
    deleteShelf: z.discriminatedUnion("__typename", [
      z.object({ __typename: z.literal("ShelfDeleted"), deletedShelfId: domainId("Shelf") }),
      refusal("SystemShelfImmutable"),
      refusal("NotFound"),
    ]),
  }),
);
editingProjections.set(
  "mutationsAddShelfItemMutation",
  z.object({
    addShelfItem: z.discriminatedUnion("__typename", [
      z.object({
        __typename: z.literal("ShelfItemAdded"),
        shelf: shelf.pick({ id: true, itemCount: true, updatedAt: true }),
        edge: z.object({ cursor: z.string(), node: z.object({ id: domainId("Manifestation") }) }),
      }),
      refusal("NotFound"),
    ]),
  }),
);
export class GraphQLExecutionError extends Error {
  readonly errors: z.infer<typeof executionError>[];

  constructor(errors: z.infer<typeof executionError>[]) {
    super(errors.map((error) => error.message).join("; "));
    this.name = "GraphQLExecutionError";
    this.errors = errors;
  }
}

const REFUSAL_ACTIONS: Readonly<Record<string, "reload" | "correct" | "none">> = {
  StaleBaseline: "reload",
  MetadataInvalid: "correct",
};

type MetadataRefusalResult = {
  readonly __typename: string;
  readonly message?: string;
  readonly fieldErrors?: readonly { readonly field: string; readonly message: string }[];
};

/** An expected metadata mutation outcome other than success, kept as data for its owning flow. */
export class MetadataRefusal extends Error {
  readonly typename: string;
  readonly fieldErrors: Readonly<Record<string, string>>;

  constructor(result: MetadataRefusalResult) {
    super(result.message ?? "Request failed.");
    this.name = "MetadataRefusal";
    this.typename = result.__typename;
    this.fieldErrors = Object.fromEntries(
      (result.fieldErrors ?? []).map(({ field, message }) => [field, message]),
    );
  }
}

export function editingFailureAction(error: unknown): "retry" | "reload" | "correct" | "none" {
  if (error instanceof MetadataRefusal) return REFUSAL_ACTIONS[error.typename] ?? "none";
  const code =
    error instanceof GraphQLExecutionError ? error.errors.at(0)?.extensions?.code : undefined;
  if (code === "if-match-mismatch" || code === "if-match-required") return "reload";
  if (code === "validation" || (error instanceof ApiError && error.status === 422))
    return "correct";
  if (error instanceof ApiError)
    return error.status >= 500 || error.status === 408 || error.status === 429 ? "retry" : "none";
  if (error instanceof GraphQLExecutionError && code !== undefined && code !== "internal")
    return "none";
  return "retry";
}

export function graphqlNetwork(
  request: RequestParameters,
  variables: Variables,
  accountSignal: AbortSignal,
): Observable<GraphQLResponse> {
  return Observable.create((sink) => {
    const controller = new AbortController();
    const signal = AbortSignal.any([controller.signal, accountSignal]);
    const cancel = () => {
      sink.complete();
    };
    if (accountSignal.aborted) {
      cancel();
      return;
    }
    if (request.id === null) {
      sink.error(new Error("A generated document ID is required."));
      return;
    }
    accountSignal.addEventListener("abort", cancel, { once: true });
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
        (editingProjections.get(request.name) ?? readerProjections.get(request.name))?.parse(
          result.data,
        );
        sink.next({ data: result.data });
        sink.complete();
      } catch (error) {
        if (signal.aborted) return;
        // THREAT: deliver the originating 401 before account cleanup completes other executions.
        sink.error(error instanceof Error ? error : new Error("GraphQL request failed."));
        if (error instanceof ApiError && error.status === 401) invokeUnauthenticatedHandler();
      }
    })();
    return () => {
      accountSignal.removeEventListener("abort", cancel);
      controller.abort();
    };
  });
}
