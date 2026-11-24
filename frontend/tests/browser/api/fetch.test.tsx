import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetCsrfTokenForTesting } from "@/api/csrf";
import { ApiError } from "@/api/errors";
import { getBookMetadata, updateBookMetadata, type BookMetadata } from "@/api/books";
import { getReadingState, updateReadingState } from "@/api/reading";
import { apiFetch } from "@/api/fetch";

import { commitMutation, createOperationDescriptor, getRequest } from "relay-runtime";
import { fetchQuery, RelayEnvironmentProvider } from "react-relay";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { AuthorsCell } from "@/pages/library/table/cells";
import { encodeGraphQLId } from "@/api/graphql";
import { forgetActiveUser, rememberActiveUser } from "@/lib/active-user";
import { relayEnvironment } from "@/lib/relay/environment";
import {
  mutationsTitleQuery,
  mutationsRevisionQuery,
  mutationsSaveTitleMutation,
} from "@/lib/relay/mutations";
import type { mutationsTitleQuery as TitleQuery } from "@/lib/relay/__generated__/mutationsTitleQuery.graphql";
import type { mutationsRevisionQuery as RevisionQuery } from "@/lib/relay/__generated__/mutationsRevisionQuery.graphql";
import type { mutationsSaveTitleMutation as SaveTitle } from "@/lib/relay/__generated__/mutationsSaveTitleMutation.graphql";
import ReadingDocument, {
  type useCellEditReadingMutation as ReadingMutation,
} from "@/lib/relay/__generated__/useCellEditReadingMutation.graphql";
import ReloadDocument, {
  type useCellEditReloadQuery,
} from "@/lib/relay/__generated__/useCellEditReloadQuery.graphql";

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

function withEtag(body: unknown, etag: string): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json", ETag: etag },
  });
}

function mismatchResponse(currentEtag: string): Response {
  return new Response(
    JSON.stringify({
      type: "https://reverie.example/probs/if-match-mismatch",
      title: "Precondition Failed",
      status: 412,
    }),
    {
      status: 412,
      headers: { "Content-Type": "application/problem+json", ETag: currentEtag },
    },
  );
}

// REV-REQ-0019 AC1-4; data loss boundary.
test("metadata and reading edits keep their baseline", async () => {
  await seedCsrf(SAMPLE_TOKEN);
  const tag = '"aaaaaaaaaaaaaaaaaaaaaa"';
  const newer = '"bbbbbbbbbbbbbbbbbbbbbb"';
  const requestId = "12345678-1234-4234-8234-123456789abc";
  const values: BookMetadata = {
    title: "Original",
    subtitle: null,
    description: null,
    language: null,
    publisher: null,
    pub_date: null,
    isbn_10: null,
    isbn_13: null,
    isbn_mismatch: false,
    pages: null,
    content_rating: null,
    genres: [],
    moods: [],
    tags: [],
    contributors: { author: [], editor: [], translator: [] },
    work_identifiers: {},
    manifestation_identifiers: {},
  };
  const reading = {
    status: null,
    rating: null,
    notes: null,
    progress_pct: null,
    started_at: null,
    finished_at: null,
    last_read_at: null,
  };
  const fetchSpy = vi.spyOn(globalThis, "fetch");
  fetchSpy.mockResolvedValueOnce(withEtag(values, tag));
  const baseline = await getBookMetadata("one");
  fetchSpy.mockResolvedValueOnce(withEtag(values, newer));
  await getBookMetadata("one");
  fetchSpy.mockResolvedValueOnce(withEtag(reading, newer));
  const readingBaseline = await getReadingState("two");
  fetchSpy.mockResolvedValueOnce(
    problemResponse({
      type: "https://reverie.example/probs/csrf-mismatch",
      title: "Forbidden",
      status: 403,
    }),
  );
  fetchSpy.mockResolvedValueOnce(jsonResponse({ csrf_token: REFRESHED_TOKEN }));
  fetchSpy.mockResolvedValueOnce(
    withEtag(
      { values: { ...values, title: "Saved" }, request_id: requestId, operations: [] },
      newer,
    ),
  );
  const saved = await updateBookMetadata("one", { title: "Saved" }, baseline.etag, requestId);
  expect(saved.values.title).toBe("Saved");
  expect(saved.etag).toBe(newer);
  const writes = fetchSpy.mock.calls.filter(([, init]) => init?.method === "PATCH");
  expect(writes).toHaveLength(2);
  for (const [, init] of writes) expect(new Headers(init?.headers).get("If-Match")).toBe(tag);
  expect(writes[0]?.[1]?.body).toBe(writes[1]?.[1]?.body);
  fetchSpy.mockResolvedValueOnce(mismatchResponse(newer));
  await expect(
    updateBookMetadata("one", { title: "Retained" }, baseline.etag, crypto.randomUUID()),
  ).rejects.toMatchObject({ status: 412 });
  fetchSpy.mockResolvedValueOnce(jsonResponse(values));
  await expect(getBookMetadata("one")).rejects.toThrow();
  fetchSpy.mockResolvedValueOnce(
    withEtag({ values: {}, request_id: requestId, operations: [] }, newer),
  );
  await expect(
    updateBookMetadata("one", { title: "Retained" }, baseline.etag, requestId),
  ).rejects.toThrow();
  fetchSpy.mockResolvedValueOnce(jsonResponse({ values, request_id: requestId, operations: [] }));
  await expect(
    updateBookMetadata("one", { title: "Retained" }, baseline.etag, requestId),
  ).rejects.toThrow();
  fetchSpy.mockResolvedValueOnce(withEtag(reading, tag));
  await getReadingState("two");
  fetchSpy.mockResolvedValueOnce(withEtag({ ...reading, rating: 4 }, tag));
  const readSaved = await updateReadingState("two", { rating: 4 }, readingBaseline.etag);
  expect(readSaved.values.rating).toBe(4);
  expect(new Headers(fetchSpy.mock.lastCall?.[1]?.headers).get("If-Match")).toBe(newer);
  fetchSpy.mockResolvedValueOnce(withEtag({ ...reading, rating: "bad" }, newer));
  await expect(updateReadingState("two", { rating: 2 }, readSaved.etag)).rejects.toThrow();
  expect(new Headers(fetchSpy.mock.lastCall?.[1]?.headers).get("If-Match")).toBe(tag);
  expect(baseline).toEqual({ values, etag: tag });
  await graphqlBaselines();
});

async function graphqlBaselines(): Promise<void> {
  rememberActiveUser("editing-account");
  const environment = relayEnvironment();
  const id = encodeGraphQLId("Manifestation", "12345678-1234-4234-8234-123456789abc");
  const workId = encodeGraphQLId("Work", "22345678-1234-4234-8234-123456789abc");
  const tag = '"aaaaaaaaaaaaaaaaaaaaaa"';
  const newer = '"bbbbbbbbbbbbbbbbbbbbbb"';
  const latest = '"cccccccccccccccccccccc"';
  const revisions = new Map([
    [tag, encodeGraphQLId("MetadataRevision", "52345678-1234-4234-8234-123456789abc")],
    [newer, encodeGraphQLId("MetadataRevision", "62345678-1234-4234-8234-123456789abc")],
    [latest, encodeGraphQLId("MetadataRevision", "72345678-1234-4234-8234-123456789abc")],
  ]);
  const revision = (etag: string) => revisions.get(etag) ?? "";
  const snapshot = { id: revision(tag), workId, manifestationId: id, etag: tag, title: "Original" };
  const fetch = vi.spyOn(globalThis, "fetch");
  fetch.mockClear();
  const titleResponse = (metadata: unknown) =>
    jsonResponse({ data: { manifestation: { id, metadata } } });
  fetch.mockResolvedValueOnce(titleResponse(snapshot));
  const initial = await fetchQuery<TitleQuery>(environment, mutationsTitleQuery, {
    id,
  }).toPromise();
  const baseline = initial?.manifestation?.metadata;
  if (baseline === undefined) throw new Error("Expected title baseline.");
  fetch.mockResolvedValueOnce(
    titleResponse({ ...snapshot, id: revision(newer), title: "Background", etag: newer }),
  );
  await fetchQuery<TitleQuery>(environment, mutationsTitleQuery, { id }).toPromise();
  fetch.mockResolvedValueOnce(
    titleResponse({ id: revision(latest), workId, manifestationId: id, etag: latest }),
  );
  const sparse = await fetchQuery<RevisionQuery>(environment, mutationsRevisionQuery, {
    id,
  }).toPromise();
  expect(sparse?.manifestation?.metadata).not.toHaveProperty("title");
  const operation = createOperationDescriptor(getRequest(mutationsTitleQuery), { id });
  expect(environment.lookup(operation.fragment).data).toMatchObject({
    manifestation: { metadata: { title: undefined, etag: latest } },
  });
  expect(baseline).toEqual(snapshot);
  fetch.mockResolvedValueOnce(
    titleResponse({ id: revision(latest), workId, manifestationId: id, title: "Completed" }),
  );
  await fetchQuery<TitleQuery>(environment, mutationsTitleQuery, { id }).toPromise();
  expect(environment.lookup(operation.fragment).data).toMatchObject({
    manifestation: { metadata: { title: "Completed", etag: latest } },
  });
  const requestId = crypto.randomUUID();
  const variables = {
    input: { id, baseline: baseline.etag, requestId, patch: { title: "Saved" } },
  };
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
          requestId,
          operations: [],
          manifestation: {
            id,
            metadata: { ...snapshot, id: revision(latest), title: "Saved", etag: latest },
          },
        },
      },
    }),
  );
  const saveTitle = () =>
    new Promise<SaveTitle["response"]>((resolve, reject) => {
      commitMutation<SaveTitle>(environment, {
        mutation: mutationsSaveTitleMutation,
        variables,
        onCompleted: resolve,
        onError: reject,
      });
    });
  const saved = await saveTitle();
  expect(saved.updateMetadata).toMatchObject({
    __typename: "MetadataSaved",
    manifestation: { metadata: { title: "Saved", etag: latest } },
  });
  const writes = fetch.mock.calls.filter(([, init]) => init?.method === "POST");
  expect(writes).toHaveLength(2);
  expect(writes[0][1]?.body).toBe(writes[1][1]?.body);
  expect(
    JSON.parse(typeof writes[0][1]?.body === "string" ? writes[0][1].body : "null"),
  ).toMatchObject({ variables });
  fetch.mockResolvedValueOnce(
    jsonResponse({
      data: { updateMetadata: { __typename: "StaleBaseline", message: "The baseline is stale." } },
    }),
  );
  await expect(saveTitle()).resolves.toMatchObject({
    updateMetadata: { __typename: "StaleBaseline" },
  });
  expect(environment.lookup(operation.fragment).data).toMatchObject({
    manifestation: { metadata: { title: "Saved", etag: latest } },
  });
  fetch.mockResolvedValueOnce(
    titleResponse({ ...snapshot, id: revision(latest), title: "Fresh decision", etag: latest }),
  );
  const fresh = await fetchQuery<TitleQuery>(environment, mutationsTitleQuery, {
    id,
  }).toPromise();
  expect(fresh?.manifestation?.metadata).toMatchObject({ title: "Fresh decision", etag: latest });
  expect(baseline).toEqual(snapshot);
  const reading = {
    id: "reading_rev_initial",
    etag: tag,
    status: null,
    rating: null,
    progressPct: null,
  };
  const metadata = {
    ...snapshot,
    id: revision(latest),
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
  const readForEdit = () =>
    fetchQuery<useCellEditReloadQuery>(environment, ReloadDocument, { id }).toPromise();
  fetch.mockResolvedValueOnce(
    jsonResponse({
      data: {
        manifestation: {
          ...manifestation,
          readingState: { id: "reading_rev_missing", status: null, rating: null },
        },
      },
    }),
  );
  await expect(readForEdit()).rejects.toThrow();
  fetch.mockResolvedValueOnce(jsonResponse({ data: { manifestation } }));
  const opened = (await readForEdit())?.manifestation?.readingState;
  if (opened === undefined) throw new Error("Expected an opening revision.");
  fetch.mockResolvedValueOnce(
    jsonResponse({
      data: {
        manifestation: {
          ...manifestation,
          readingState: { ...reading, id: "reading_rev_background", etag: newer },
        },
      },
    }),
  );
  await readForEdit();
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
  expect(
    JSON.parse(
      typeof fetch.mock.lastCall?.[1]?.body === "string" ? fetch.mock.lastCall[1].body : "null",
    ),
  ).toMatchObject({ variables: { input: { baseline: tag } } });

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
        baseline={tag}
        density="comfortable"
        state={{ pending: undefined, failed: false }}
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
}
