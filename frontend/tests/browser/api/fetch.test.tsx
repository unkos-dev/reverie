import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "@/api/csrf";
import { ApiError } from "@/api/errors";
import { apiFetch } from "@/api/fetch";

import {
  commitMutation,
  createOperationDescriptor,
  getPluralSelector,
  getRequest,
  getSingularSelector,
} from "relay-runtime";
import { fetchQuery, RelayEnvironmentProvider } from "react-relay";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { z } from "zod";
import { AuthorsCell } from "@/pages/library/table/cells";
import { forgetActiveUser, rememberActiveUser } from "@/lib/active-user";
import { relayEnvironment } from "@/lib/relay/environment";
import OpeningDocument, {
  type EditMetadataDialogQuery as OpeningQuery,
} from "@/lib/relay/__generated__/EditMetadataDialogQuery.graphql";
import OpeningFragment from "@/lib/relay/__generated__/EditMetadataDialog_opening.graphql";
import AuthorsDocument, {
  type cellsAuthorsQuery as AuthorsQuery,
} from "@/lib/relay/__generated__/cellsAuthorsQuery.graphql";
import SaveDocument, {
  type EditMetadataDialogSaveMutation as SaveMutation,
} from "@/lib/relay/__generated__/EditMetadataDialogSaveMutation.graphql";
import ReadingDocument, {
  type useCellEditReadingMutation as ReadingMutation,
} from "@/lib/relay/__generated__/useCellEditReadingMutation.graphql";
import LibraryDocument, {
  type LibraryPageRowsPaginationQuery as LibraryQuery,
} from "@/lib/relay/__generated__/LibraryPageRowsPaginationQuery.graphql";
import RowsFragment from "@/lib/relay/__generated__/LibraryPage_rows.graphql";
import BookCard from "@/lib/relay/__generated__/BookCard_book.graphql";
import BatchRows from "@/lib/relay/__generated__/useCellEdit_batchRows.graphql";

const SAMPLE_TOKEN = "abcdefghijklmnopqrstuvwxyz0123456789_-ABCDE"; // 43 chars
const REFRESHED_TOKEN = "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"; // 43 Z

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

function problemResponse(
  problem: {
    type: string;
    title: string;
    status: number;
    detail?: string;
    errors?: Record<string, string>;
  },
  init?: ResponseInit,
): Response {
  return new Response(JSON.stringify(problem), {
    status: problem.status,
    headers: { "Content-Type": "application/problem+json" },
    ...init,
  });
}

beforeEach(() => {
  __resetCsrfTokenForTesting();
  vi.restoreAllMocks();
});

afterEach(() => {
  forgetActiveUser();
  __resetCsrfTokenForTesting();
});

async function seedCsrf(token: string): Promise<void> {
  const fetchSpy = vi
    .spyOn(globalThis, "fetch")
    .mockResolvedValueOnce(jsonResponse({ csrf_token: token }));
  const mod = await import("@/api/csrf");
  await mod.refreshCsrfToken();
  fetchSpy.mockRestore();
}

describe("apiFetch — CSRF header injection", () => {
  test.each(["POST", "PUT", "PATCH", "DELETE"])(
    "%s injects X-CSRF-Token when token is cached",
    async (method) => {
      await seedCsrf(SAMPLE_TOKEN);
      const fetchSpy = vi
        .spyOn(globalThis, "fetch")
        .mockResolvedValueOnce(jsonResponse({ ok: true }));

      await apiFetch("/api/v1/books", { method, body: "{}" });

      const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
      expect(headers.get("X-CSRF-Token")).toBe(SAMPLE_TOKEN);
    },
  );

  test("POST with no cached token hydrates once; still omits the header when none is issued", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({}))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    await apiFetch("/api/v1/books", { method: "POST" });

    expect(fetchSpy).toHaveBeenCalledTimes(2);
    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/auth/me");
    const headers = new Headers(fetchSpy.mock.calls[1]?.[1]?.headers);
    expect(headers.has("X-CSRF-Token")).toBe(false);
  });
});

describe("apiFetch — CSRF rejection retry", () => {
  test.each([
    [403, "csrf-mismatch"],
    [403, "csrf-missing"],
    [428, "csrf-missing"],
    [428, "csrf-mismatch"],
  ])("a %i %s rejection refreshes once and retries with the new token", async (status, slug) => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        problemResponse({
          type: `https://reverie.example/probs/${slug}`,
          title: "Rejected",
          status,
        }),
      )
      .mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }))
      .mockResolvedValueOnce(jsonResponse({ ok: true }));

    const result = await apiFetch("/api/v1/books", { method: "POST", body: "{}" });

    expect(result).toEqual({ ok: true });
    expect(fetchSpy).toHaveBeenCalledTimes(3);
    expect(fetchSpy.mock.calls[1]?.[0]).toBe("/auth/me");
    const retriedHeaders = new Headers(fetchSpy.mock.calls[2]?.[1]?.headers);
    expect(retriedHeaders.get("X-CSRF-Token")).toBe(REFRESHED_TOKEN);
  });

  test("an empty refreshed token drops the stale header, and the retry's rejection reaches the caller", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(
        problemResponse({
          type: "https://reverie.example/probs/csrf-mismatch",
          title: "Forbidden",
          status: 403,
        }),
      )
      .mockResolvedValueOnce(jsonResponse({ csrf_token: "" }))
      .mockResolvedValueOnce(
        problemResponse({
          type: "https://reverie.example/probs/csrf-missing",
          title: "Precondition Required",
          status: 428,
        }),
      );

    const err = (await apiFetch("/api/v1/books", { method: "POST", body: "{}" }).catch(
      (e: unknown) => e,
    )) as ApiError;

    expect(err.status).toBe(428);
    expect(err.problemSlug).toBe("csrf-missing");
    expect(fetchSpy).toHaveBeenCalledTimes(3);
    expect(new Headers(fetchSpy.mock.calls[2]?.[1]?.headers).has("X-CSRF-Token")).toBe(false);
  });

  test("a non-CSRF 428 throws without a retry", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/some-other-precondition",
        title: "Precondition Required",
        status: 428,
      }),
    );

    await expect(apiFetch("/api/v1/books", { method: "POST", body: "{}" })).rejects.toThrow(
      ApiError,
    );
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});

describe("apiFetch — RFC 9457 error parsing", () => {
  test("401 unauthorized → ApiError with parsed type/title/detail", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/unauthorized",
        title: "Unauthorized",
        status: 401,
        detail: "Authentication required.",
      }),
    );

    const err = await apiFetch("/api/v1/books").catch((e: unknown) => e);
    expect(err).toBeInstanceOf(ApiError);
    const apiErr = err as ApiError;
    expect(apiErr.status).toBe(401);
    expect(apiErr.type).toBe("https://reverie.example/probs/unauthorized");
    expect(apiErr.title).toBe("Unauthorized");
    expect(apiErr.detail).toBe("Authentication required.");
    expect(apiErr.problemSlug).toBe("unauthorized");
    expect(apiErr.errors).toBeUndefined();
  });

  test("422 validation preserves field errors", async () => {
    const problem = {
      type: "https://reverie.example/probs/validation",
      title: "Unprocessable Entity",
      status: 422,
      detail: "Invalid publication date.",
      errors: { pub_date: "Use YYYY-MM-DD or YYYY. Leave empty if unknown." },
    };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(problemResponse(problem));
    const error = await apiFetch("/api/v1/books").catch((error: unknown) => error);
    expect(error).toBeInstanceOf(ApiError);
    if (!(error instanceof ApiError)) throw new Error("Expected ApiError");
    expect(error.status).toBe(problem.status);
    expect(error.type).toBe(problem.type);
    expect(error.title).toBe(problem.title);
    expect(error.detail).toBe(problem.detail);
    expect(error.errors).toEqual(problem.errors);
  });

  test("500 internal → ApiError with title='Internal Server Error'", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/internal",
        title: "Internal Server Error",
        status: 500,
        detail: "An internal error occurred.",
      }),
    );

    const err = (await apiFetch("/api/v1/books").catch((e: unknown) => e)) as ApiError;
    expect(err.status).toBe(500);
    expect(err.title).toBe("Internal Server Error");
  });

  test("non-JSON error body falls back to status-text title", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response("Gateway down", {
        status: 502,
        statusText: "Bad Gateway",
        headers: { "Content-Type": "text/plain" },
      }),
    );

    const err = (await apiFetch("/api/v1/books").catch((e: unknown) => e)) as ApiError;
    expect(err.status).toBe(502);
    expect(err.title).toBe("Bad Gateway");
    expect(err.type).toBeNull();
    expect(err.problemSlug).toBeNull();
  });

  test("other JSON and empty error bodies keep their status", async () => {
    for (const response of [
      jsonResponse({ error: "conflict" }, { status: 409, statusText: "Conflict" }),
      new Response(null, { status: 503, statusText: "Service Unavailable" }),
    ]) {
      vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(response);
      const err = await apiFetch("/api/v1/books").catch((e: unknown) => e);
      expect(err).toBeInstanceOf(ApiError);
      if (!(err instanceof ApiError)) throw new Error("Expected ApiError");
      expect(err.status).toBe(response.status);
      expect(err.title).toBe(response.statusText);
      expect(err.type).toBeNull();
    }
  });
});

describe("apiFetch — non-CSRF 403", () => {
  test("403 with a NON-csrf-mismatch problem type does not retry", async () => {
    await seedCsrf(SAMPLE_TOKEN);

    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/forbidden",
        title: "Forbidden",
        status: 403,
        detail: "Access denied.",
      }),
    );

    const err = (await apiFetch("/api/v1/books", { method: "POST" }).catch(
      (e: unknown) => e,
    )) as ApiError;
    expect(err.status).toBe(403);
    expect(err.problemSlug).toBe("forbidden");
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });
});

describe("apiFetch — body decoding", () => {
  test("204 on csrf-mismatch retry path returns undefined (does not call .json())", async () => {
    await seedCsrf(SAMPLE_TOKEN);
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    // Attempt 1: 403 csrf-mismatch.
    fetchSpy.mockResolvedValueOnce(
      problemResponse({
        type: "https://reverie.example/probs/csrf-mismatch",
        title: "Forbidden",
        status: 403,
      }),
    );
    // /auth/me refresh.
    fetchSpy.mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }));
    // Retry: succeeds with 204 (typical for DELETE after rotation).
    fetchSpy.mockResolvedValueOnce(new Response(null, { status: 204 }));

    const result = await apiFetch("/api/v1/books/1", { method: "DELETE" });
    expect(result).toBeUndefined();
    expect(fetchSpy).toHaveBeenCalledTimes(3);
  });
});

// REV-REQ-0019 and REV-REQ-0083. Keep validators paired with complete values from one revision.
test("metadata and reading edits keep their baseline", async () => {
  await seedCsrf(SAMPLE_TOKEN);
  rememberActiveUser("editing-account");
  const environment = relayEnvironment();
  const id = "editing-book";
  const tag = '"aaaaaaaaaaaaaaaaaaaaaa"';
  const newer = '"bbbbbbbbbbbbbbbbbbbbbb"';
  const latest = '"cccccccccccccccccccccc"';
  const revisions = new Map([
    [tag, "opening-revision"],
    [newer, "background-revision"],
    [latest, "latest-revision"],
  ]);
  const revision = (etag: string) => revisions.get(etag) ?? "";
  const snapshot = { id: revision(tag), etag: tag, title: "Original" };
  const formValues = {
    subtitle: null,
    description: null,
    language: null,
    publisher: null,
    publicationDate: null,
    isbn10: null,
    isbn13: null,
    isbnMismatch: false,
    pages: null,
  };
  const fetch = vi.spyOn(globalThis, "fetch");
  fetch.mockClear();
  const titleResponse = (metadata: unknown) =>
    jsonResponse({ data: { manifestation: { id, work: null, metadata } } });
  const initialQuery = () =>
    fetchQuery<OpeningQuery>(environment, OpeningDocument, { id }).toPromise();
  const readOpening = (key: unknown) =>
    environment.lookup(getSingularSelector(OpeningFragment, key));
  const operation = createOperationDescriptor(getRequest(OpeningDocument), { id });
  function currentOpening() {
    const query = z
      .object({ manifestation: z.object({ metadata: z.unknown() }) })
      .parse(environment.lookup(operation.fragment).data);
    return readOpening(query.manifestation.metadata);
  }
  fetch.mockResolvedValueOnce(titleResponse({ ...formValues, ...snapshot }));
  const initial = await initialQuery();
  const baseline = z
    .object({ etag: z.string(), title: z.string() })
    .parse(readOpening(initial?.manifestation?.metadata).data);
  fetch.mockResolvedValueOnce(
    titleResponse({ ...formValues, id: revision(newer), title: "Background", etag: newer }),
  );
  await initialQuery();
  fetch.mockResolvedValueOnce(
    titleResponse({
      id: revision(latest),
      etag: latest,
      authors: { edges: [], pageInfo: { hasNextPage: false, endCursor: null } },
    }),
  );
  await fetchQuery<AuthorsQuery>(environment, AuthorsDocument, { id }).toPromise();
  expect(currentOpening().isMissingData).toBe(true);
  expect(currentOpening().data).toMatchObject({ title: undefined, etag: latest });
  fetch.mockResolvedValueOnce(
    titleResponse({ ...formValues, id: revision(latest), title: "Completed" }),
  );
  await initialQuery();
  expect(currentOpening().isMissingData).toBe(false);
  expect(currentOpening().data).toMatchObject({ title: "Completed", etag: latest });
  const variables = { input: { id, baseline: baseline.etag, patch: { title: "Saved" } } };
  fetch.mockClear();
  fetch.mockResolvedValueOnce(
    problemResponse({
      type: "https://reverie.example/probs/csrf-mismatch",
      title: "Forbidden",
      status: 403,
    }),
  );
  fetch.mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }));
  fetch.mockResolvedValueOnce(
    jsonResponse({
      data: {
        updateMetadata: {
          __typename: "MetadataSaved",
          manifestation: {
            id,
            metadata: { ...formValues, id: "saved-revision", title: "Saved", etag: latest },
            work: null,
            coverUrl: "",
            createdAt: "2026-01-01T00:00:00Z",
            readingState: { id: "unchanged-reading", etag: tag, status: null, rating: null },
            isbn10: null,
            isbn13: null,
            isbnMismatch: false,
            ingestionStatus: "COMPLETE",
            validationStatus: "CLEAN",
            enrichmentStatus: "COMPLETE",
            tags: { edges: [], pageInfo: { hasNextPage: false, endCursor: null } },
            publisher: null,
            publicationDate: null,
            versions: {
              pendingCount: 0,
              pending: { edges: [], pageInfo: { hasNextPage: false, endCursor: null } },
            },
          },
        },
      },
    }),
  );
  const saved = await new Promise<SaveMutation["response"]>((resolve, reject) => {
    commitMutation<SaveMutation>(environment, {
      mutation: SaveDocument,
      variables,
      onCompleted: resolve,
      onError: reject,
    });
  });
  expect(saved.updateMetadata.__typename).toBe("MetadataSaved");
  expect(currentOpening().data).toMatchObject({ title: "Saved", etag: latest });
  const writes = fetch.mock.calls.filter(([, init]) => init?.method === "POST");
  expect(writes).toHaveLength(2);
  expect(writes[0][1]?.body).toBe(writes[1][1]?.body);
  expect(
    JSON.parse(typeof writes[0][1]?.body === "string" ? writes[0][1].body : "null"),
  ).toMatchObject({ variables: { input: { id, baseline: tag, patch: { title: "Saved" } } } });

  const reading = {
    id: "reading_rev_initial",
    etag: tag,
    status: null,
    rating: null,
    progressPct: null,
  };
  const metadata = {
    ...snapshot,
    id: "saved-revision",
    title: "Saved",
    etag: latest,
    subtitle: null,
    isbn13: null,
    pages: null,
  };
  const manifestation = {
    id,
    metadata,
    readingState: reading,
    coverUrl: "",
    work: null,
    createdAt: "2026-01-01T00:00:00Z",
  };
  function libraryResponse(row: unknown): Response {
    return jsonResponse({
      data: {
        books: {
          edges: [{ cursor: "one", node: row }],
          pageInfo: {
            hasNextPage: false,
            hasPreviousPage: false,
            startCursor: "one",
            endCursor: "one",
          },
        },
      },
    });
  }
  const readForEdit = async () => {
    const query = await fetchQuery<LibraryQuery>(environment, LibraryDocument, {
      sourceKey: "editing",
    }).toPromise();
    const connection = environment.lookup(getSingularSelector(RowsFragment, query)).data;
    const node = z
      .object({
        books: z.object({
          edges: z.array(z.object({ node: z.unknown() })),
        }),
      })
      .parse(connection).books.edges[0]?.node;
    const selector = getPluralSelector(BatchRows, [node]).selectors[0];
    const batch = environment.lookup(selector);
    if (batch.isMissingData) throw new Error("Incomplete reading revision.");
    return z.object({ readingState: z.object({ etag: z.string() }) }).parse(batch.data)
      .readingState;
  };
  fetch.mockResolvedValueOnce(
    libraryResponse({
      ...manifestation,
      readingState: { id: "reading_rev_missing", status: null, rating: null, progressPct: null },
    }),
  );
  await expect(readForEdit()).rejects.toThrow("Incomplete reading revision.");
  fetch.mockResolvedValueOnce(libraryResponse(manifestation));
  const opened = await readForEdit();
  fetch.mockResolvedValueOnce(
    libraryResponse({
      ...manifestation,
      readingState: { ...reading, id: "reading_rev_background", etag: newer },
    }),
  );
  await readForEdit();
  fetch.mockClear();
  fetch.mockResolvedValueOnce(
    problemResponse({
      type: "https://reverie.example/probs/csrf-mismatch",
      title: "Forbidden",
      status: 403,
    }),
  );
  fetch.mockResolvedValueOnce(jsonResponse({ csrf_token: SAMPLE_TOKEN }));
  fetch.mockResolvedValueOnce(
    jsonResponse({
      data: {
        updateReadingState: {
          __typename: "ReadingStateUpdated",
          manifestation: {
            ...manifestation,
            readingState: { ...reading, id: "reading_rev_saved", rating: 3, etag: latest },
          },
        },
      },
    }),
  );
  await new Promise<ReadingMutation["response"]>((resolve, reject) => {
    commitMutation<ReadingMutation>(environment, {
      mutation: ReadingDocument,
      variables: { input: { id, baseline: opened.etag, patch: { rating: 3 } } },
      onCompleted: resolve,
      onError: reject,
    });
  });
  const readingWrites = fetch.mock.calls.filter(([, init]) => init?.method === "POST");
  expect(readingWrites).toHaveLength(2);
  expect(readingWrites[0][1]?.body).toBe(readingWrites[1][1]?.body);
  const readingBody = readingWrites[0][1]?.body;
  expect(JSON.parse(typeof readingBody === "string" ? readingBody : "null")).toMatchObject({
    variables: { input: { baseline: tag } },
  });
  const library = createOperationDescriptor(getRequest(LibraryDocument), { sourceKey: "editing" });
  const savedConnection = environment.lookup(
    getSingularSelector(RowsFragment, environment.lookup(library.fragment).data),
  ).data;
  const savedNode = z
    .object({ books: z.object({ edges: z.array(z.object({ node: z.unknown() })) }) })
    .parse(savedConnection).books.edges[0]?.node;
  const savedReading = environment.lookup(getPluralSelector(BatchRows, [savedNode]).selectors[0]);
  expect(savedReading.data).toMatchObject({ readingState: { etag: latest } });
  const savedCard = environment.lookup(getSingularSelector(BookCard, savedNode));
  expect(savedCard.data).toMatchObject({ readingState: { rating: 3 } });

  const authorPage = (
    etag: string,
    name: string,
    cursor: string,
    hasNextPage: boolean,
    continuation = false,
  ) => ({
    data: {
      [continuation ? "fetch__Manifestation" : "manifestation"]: {
        id,
        metadata: {
          id: revision(etag),
          etag,
          authors: {
            edges: [{ cursor, node: { name } }],
            pageInfo: { hasNextPage, endCursor: cursor },
          },
        },
      },
    },
  });
  fetch.mockClear();
  fetch.mockResolvedValueOnce(jsonResponse(authorPage(tag, "Old author", "old-one", true)));
  let refuseOld: ((value: Response) => void) | undefined;
  fetch.mockReturnValueOnce(
    new Promise<Response>((resolve) => {
      refuseOld = resolve;
    }),
  );
  fetch.mockResolvedValueOnce(jsonResponse(authorPage(newer, "Alice", "new-one", true, true)));
  fetch.mockResolvedValueOnce(jsonResponse(authorPage(newer, "Bob", "new-two", false, true)));

  const onCommit = vi.fn();
  await render(
    <RelayEnvironmentProvider environment={environment}>
      <AuthorsCell
        id={id}
        authors={["Old author"]}
        density="comfortable"
        state={{ pending: undefined }}
        onCommit={onCommit}
      />
    </RelayEnvironmentProvider>,
  );
  await userEvent.click(page.getByRole("button", { name: /edit authors/i }));
  await vi.waitFor(() => {
    expect(fetch).toHaveBeenCalledTimes(2);
  });
  expect(page.getByRole("button", { name: "Save", exact: true }).elements()).toHaveLength(0);
  expect(onCommit).not.toHaveBeenCalled();
  refuseOld?.(
    jsonResponse({ errors: [{ message: "Stale cursor", extensions: { code: "invalid-cursor" } }] }),
  );
  await expect
    .element(page.getByRole("dialog", { name: "Edit authors" }).getByRole("row", { name: "Bob" }))
    .toBeInTheDocument();
  expect(fetch).toHaveBeenCalledTimes(4);
  await userEvent.fill(page.getByRole("textbox", { name: "Add an author" }), "Dee");
  await userEvent.click(page.getByRole("button", { name: "Save", exact: true }));
  expect(onCommit).toHaveBeenCalledWith(["Alice", "Bob", "Dee"], newer, ["Alice", "Bob"]);
});
