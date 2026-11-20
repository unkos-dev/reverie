import type { readerLibraryBook$data } from "@/lib/relay/__generated__/readerLibraryBook.graphql";
import type { LibraryRowSnapshot } from "@/lib/relay/library-visit";
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
import {
  ContentRatingSchema,
  ReadingStatusSchema,
  ResourceEtagSchema,
  parseSortParam,
  type BookListItem,
  type ListBooksParams,
  type SortField,
} from "./books";
import type {
  SortField as GraphQLSortField,
  readerLibraryQuery$variables,
} from "@/lib/relay/__generated__/readerLibraryQuery.graphql";
import type { mutationsAuthorPageQuery } from "@/lib/relay/__generated__/mutationsAuthorPageQuery.graphql";
import { mutationsAuthorPageQuery as authorPageDocument } from "@/lib/relay/mutations";
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
  MetadataVersion: "metadata_version",
  Mood: "mood",
  Publisher: "publisher",
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
const revision = z.object({
  workId: domainId("Work"),
  manifestationId: domainId("Manifestation"),
  etag: ResourceEtagSchema,
});
const titleSnapshot = revision.extend({ title: z.string() });
const cellsSnapshot = titleSnapshot.extend({
  subtitle: z.string().nullable(),
  isbn13: z.string().nullable(),
  pages: z.number().int().nullable(),
});
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
  ["readerShelvesQuery", z.object({ shelves: connection(shelfProjection) })],
  ["readerShelvesPaginationQuery", z.object({ shelves: connection(shelfProjection) })],
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
for (const kind of ["Author", "Series", "Genre", "Mood", "Tag", "Publisher"] as const) {
  for (const action of kind === "Author" || kind === "Series"
    ? ["Suggestions", "Labels"]
    : ["Suggestions"]) {
    readerProjections.set(
      `reader${kind}${action}Query`,
      z.object({
        [`${kind.toLowerCase()}${action}`]: z.array(
          z.object({ entityId: domainId(kind).nullable() }),
        ),
      }),
    );
  }
}
const authorSnapshot = revision.extend({
  authors: z.object({
    edges: z.array(
      z.object({
        cursor: z.string(),
        node: z.object({ name: z.string(), author: z.object({ id: domainId("Author") }) }),
      }),
    ),
    pageInfo: pageInfo.refine(
      (page) => !page.hasNextPage || page.endCursor !== null,
      "An author continuation cursor is required.",
    ),
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
  ["Cells", cellsSnapshot],
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
  ["Cells", cellsSnapshot],
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

const libraryPage = z.object({
  books: z.object({
    edges: z.array(
      z.object({
        cursor: z.string(),
        node: z.object({
          id: domainId("Manifestation"),
          createdAt: z.iso.datetime(),
          work: workProjection
            .extend({
              selectedSeries: z.object({ node: z.object({ id: domainId("Series") }) }).nullable(),
            })
            .nullable(),
          metadata: cellsSnapshot,
          readingState: z.object({
            etag: ResourceEtagSchema,
            status: ReadingStatusSchema.nullable(),
            rating: z.number().int().min(1).max(5).nullable(),
            progressPct: z.number().nullable(),
            startedAt: z.iso.datetime().nullable(),
            finishedAt: z.iso.datetime().nullable(),
          }),
        }),
      }),
    ),
    pageInfo: pageInfo
      .pick({ hasNextPage: true, endCursor: true })
      .refine(
        (page) => !page.hasNextPage || page.endCursor !== null,
        "A Library continuation cursor is required.",
      ),
  }),
});
editingProjections.set("readerLibraryQuery", libraryPage);
editingProjections.set("readerLibraryPaginationQuery", libraryPage);

type EditingResponse<T extends OperationType> = T extends { readonly rawResponse: infer Raw }
  ? Raw
  : T["response"];

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
const refusal = <T extends string>(typename: T) =>
  z.object({ __typename: z.literal(typename), message: z.string() });
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
  "mutationsRefreshMetadataMutation",
  z.object({ refreshMetadata: z.object({ manifestationId: domainId("Manifestation") }) }),
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
): Observable<EditingResponse<T>> {
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
    // The network validates the operation's selected projection before publishing this raw payload.
    return structuredClone(payload.data) as EditingResponse<T>;
  });
}

/** Await one validated raw payload and settle disposal without acknowledging cancellation. */
export function executeEditingOnce<T extends OperationType>(
  environment: Environment,
  document: GraphQLTaggedNode,
  variables: T["variables"],
  signal?: AbortSignal,
  capture?: (payload: EditingResponse<T>) => void,
): Promise<EditingResponse<T> | undefined> {
  return Observable.create<EditingResponse<T>>((sink) => {
    if (signal?.aborted) {
      sink.complete();
      return;
    }
    const cancel = () => {
      sink.complete();
    };
    signal?.addEventListener("abort", cancel, { once: true });
    const subscription = executeEditing<T>(environment, document, variables)
      .map((payload) => {
        capture?.(payload);
        return payload;
      })
      .subscribe(sink);
    return () => {
      signal?.removeEventListener("abort", cancel);
      subscription.unsubscribe();
    };
  }).toPromise();
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
    authors.push(
      ...snapshot.authors.edges.map(({ node }) => ({ id: node.author.id, name: node.name })),
    );
    after = snapshot.authors.pageInfo.endCursor;
    hasNext = snapshot.authors.pageInfo.hasNextPage;
    if (hasNext && after === null) throw new Error("An author continuation cursor is required.");
  }
  if (hasNext) throw new Error("Complete author pages are required.");
  return { workId: base.workId, manifestationId: base.manifestationId, etag: base.etag, authors };
}

/** Load the complete author field at one revision before allowing replacement. */
export async function loadAuthorField(
  environment: Environment,
  id: string,
  signal?: AbortSignal,
): Promise<ReturnType<typeof captureAuthorPages> | undefined> {
  const pages: AuthorPage[] = [];
  let after: string | null = null;
  let hasNext: boolean;
  do {
    const response: mutationsAuthorPageQuery["response"] | undefined =
      await executeEditingOnce<mutationsAuthorPageQuery>(
        environment,
        authorPageDocument,
        {
          id,
          first: 20,
          after,
        },
        signal,
      );
    if (response === undefined) return undefined;
    const snapshot: AuthorPage["snapshot"] | undefined = response.manifestation?.metadata;
    if (snapshot === undefined) throw new Error("Not found");
    pages.push({ after, snapshot });
    after = snapshot.authors.pageInfo.endCursor ?? null;
    hasNext = snapshot.authors.pageInfo.hasNextPage;
  } while (hasNext);
  return captureAuthorPages(pages);
}

export class GraphQLExecutionError extends Error {
  readonly errors: z.infer<typeof executionError>[];

  constructor(errors: z.infer<typeof executionError>[]) {
    super(errors.map((error) => error.message).join("; "));
    this.name = "GraphQLExecutionError";
    this.errors = errors;
  }
}

export function editingFailureAction(error: unknown): "retry" | "reload" | "correct" | "none" {
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

const SORT_COLUMNS = {
  title: "TITLE",
  author: "AUTHOR",
  created_at: "CREATED_AT",
  pages: "PAGES",
} as const satisfies Record<SortField, GraphQLSortField>;

export function libraryVariables(
  params: ListBooksParams,
  sourceKey: string,
): readerLibraryQuery$variables {
  const filter = {
    q: params.q,
    author: params.author,
    authorAny: params.author_any,
    authorNone: params.author_none,
    series: params.series,
    seriesAny: params.series_any,
    shelf: params.shelf,
    shelfAny: params.shelf_any,
    tag: params.tag,
    tagAny: params.tag_any,
    tagNone: params.tag_none,
    genre: params.genre,
    genreAny: params.genre_any,
    genreNone: params.genre_none,
    mood: params.mood,
    moodAny: params.mood_any,
    moodNone: params.mood_none,
    statusAny: params.status_any,
    statusNone: params.status_none,
    titleContains: params.title_contains,
    titleEq: params.title_eq,
    titleNe: params.title_ne,
    subtitleContains: params.subtitle_contains,
    subtitleEmpty: params.subtitle_empty,
    isbn13Contains: params.isbn_13_contains,
    isbn13Eq: params.isbn_13_eq,
    isbn13Empty: params.isbn_13_empty,
    pagesGte: params.pages_gte,
    pagesLte: params.pages_lte,
    pagesEmpty: params.pages_empty,
    ratingGte: params.rating_gte,
    ratingLte: params.rating_lte,
    ratingEmpty: params.rating_empty,
    createdAtGte: params.created_at_gte,
    createdAtLt: params.created_at_lt,
  };
  return {
    sourceKey,
    count: 50,
    filter,
    ...(params.sort === undefined || params.sort === ""
      ? {}
      : {
          sort: parseSortParam(params.sort).map((level) => ({
            column: SORT_COLUMNS[level.field],
            direction: level.desc ? "DESC" : "ASC",
          })),
        }),
  };
}

export type ReaderBook = Pick<
  BookListItem,
  | "id"
  | "work_id"
  | "metadata_etag"
  | "reading_etag"
  | "title"
  | "subtitle"
  | "authors"
  | "series"
  | "isbn_13"
  | "pages"
  | "cover_url"
  | "reading_state"
  | "created_at"
>;

export function readerBook(
  data: readerLibraryBook$data[number],
  snapshot: LibraryRowSnapshot,
): ReaderBook {
  const series = data.work?.selectedSeries;
  return {
    id: data.id,
    work_id: snapshot.metadata.workId,
    metadata_etag: snapshot.metadata.etag,
    reading_etag: snapshot.readingState.etag,
    title: snapshot.metadata.title,
    subtitle: snapshot.metadata.subtitle ?? null,
    isbn_13: snapshot.metadata.isbn13 ?? null,
    pages: snapshot.metadata.pages ?? null,
    authors: data.work?.authors.edges.map((edge) => edge.node.name) ?? [],
    series:
      series == null
        ? null
        : {
            id: series.node.id,
            name: series.node.name,
            position: series.position ?? null,
          },
    cover_url: data.coverUrl,
    created_at: data.createdAt,
    reading_state: {
      status: ReadingStatusSchema.nullable().parse(snapshot.readingState.status),
      rating: snapshot.readingState.rating ?? null,
      progress_pct: snapshot.readingState.progressPct ?? null,
      started_at: snapshot.readingState.startedAt ?? null,
      finished_at: snapshot.readingState.finishedAt ?? null,
    },
  };
}
