import { afterEach, expect, test, vi } from "vite-plus/test";
import { fetchQuery, getRequest } from "relay-runtime";

import { GraphQLExecutionError, graphqlNetwork } from "@/api/graphql";
import { ApiError } from "@/api/errors";
import { __seedCsrfTokenForTesting } from "@/api/csrf";
import {
  activeUserEpoch,
  activeUserId,
  forgetActiveUser,
  rememberActiveUser,
} from "@/lib/active-user";
import { setUnauthenticatedHandler } from "@/lib/query/client";
import { relayEnvironment } from "@/lib/relay/environment";
import { mutationsSaveTitleMutation } from "@/lib/relay/mutations";
import { viewerQuery } from "@/lib/relay/viewer";
import type { viewerQuery as ViewerQuery } from "@/lib/relay/__generated__/viewerQuery.graphql";

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function request(): Promise<ViewerQuery["response"] | undefined> {
  return fetchQuery<ViewerQuery>(
    relayEnvironment(),
    viewerQuery,
    {},
    { fetchPolicy: "network-only" },
  ).toPromise();
}

afterEach(() => {
  forgetActiveUser();
  setUnauthenticatedHandler(() => {});
  vi.restoreAllMocks();
});

test("GraphQL network preserves failures, CSRF recovery and account isolation", async () => {
  rememberActiveUser("first-account");
  const fetch = vi.spyOn(globalThis, "fetch");
  const data = {
    viewer: { id: "User:first-account", displayName: "First reader", isChild: false },
  };
  fetch.mockResolvedValueOnce(json({ data }));
  expect(await request()).toEqual(data);
  const first = fetch.mock.calls[0];
  expect(first[0]).toBe("/api/graphql");
  expect(first[1]?.credentials).toBe("same-origin");
  expect(JSON.parse(typeof first[1]?.body === "string" ? first[1].body : "null")).toEqual({
    id: getRequest(viewerQuery).params.id,
    variables: {},
  });
  for (const body of [
    json(
      {
        type: "https://reverie.example/probs/validation",
        title: "Validation",
        detail: "Invalid",
        errors: { name: "Required" },
      },
      422,
    ),
    new Response("proxy error", { status: 502 }),
    new Response(null, { status: 503 }),
    json({ other: true }, 409),
  ]) {
    fetch.mockResolvedValueOnce(body);
    await expect(request()).rejects.toMatchObject({ status: body.status });
  }
  fetch.mockResolvedValueOnce(
    json(
      {
        type: "https://reverie.example/probs/validation",
        title: "Validation",
        detail: "Invalid",
        errors: { name: "Required" },
      },
      422,
    ),
  );
  await expect(request()).rejects.toMatchObject({
    status: 422,
    title: "Validation",
    detail: "Invalid",
    errors: { name: "Required" },
  });
  __seedCsrfTokenForTesting("old-token");
  fetch.mockClear();
  fetch
    .mockResolvedValueOnce(json({ type: "https://reverie.example/probs/csrf-mismatch" }, 403))
    .mockResolvedValueOnce(json({ csrf_token: "fresh-token" }))
    .mockResolvedValueOnce(json({ data }));
  expect(await request()).toEqual(data);
  expect(fetch).toHaveBeenCalledTimes(3);
  expect(fetch.mock.calls[0]?.[1]?.body).toBe(fetch.mock.calls[2]?.[1]?.body);
  expect(new Headers(fetch.mock.calls[2]?.[1]?.headers).get("X-CSRF-Token")).toBe("fresh-token");
  fetch.mockClear();
  fetch
    .mockResolvedValueOnce(json({ type: "https://reverie.example/probs/csrf-missing" }, 428))
    .mockResolvedValueOnce(json({ csrf_token: "fresh-token" }))
    .mockResolvedValueOnce(json({ type: "https://reverie.example/probs/csrf-missing" }, 428));
  await expect(request()).rejects.toBeInstanceOf(ApiError);
  expect(fetch).toHaveBeenCalledTimes(3);
  fetch.mockClear();
  fetch.mockResolvedValueOnce(
    json({
      data,
      errors: [
        {
          message: "Refused",
          extensions: {
            code: "field-locked",
            lockedFields: ["title"],
            currentEtag: '"aaaaaaaaaaaaaaaaaaaaaa"',
            errors: { name: "Required" },
          },
        },
      ],
    }),
  );
  await expect(request()).rejects.toMatchObject({
    errors: [
      {
        message: "Refused",
        extensions: {
          code: "field-locked",
          lockedFields: ["title"],
          currentEtag: '"aaaaaaaaaaaaaaaaaaaaaa"',
          errors: { name: "Required" },
        },
      },
    ],
  });
  expect(fetch).toHaveBeenCalledTimes(1);
  fetch.mockResolvedValueOnce(
    json({
      errors: [{ message: "An internal error occurred.", extensions: { code: "internal" } }],
    }),
  );
  await expect(request()).rejects.toBeInstanceOf(GraphQLExecutionError);
  for (const malformed of [{}, { errors: [] }, { errors: [{ message: 7 }] }, { data: null }]) {
    fetch.mockResolvedValueOnce(json(malformed));
    await expect(request()).rejects.toThrow();
  }
  const id = btoa("Manifestation:12345678-1234-4234-8234-123456789abc");
  const operation = {
    operationId: "22345678-1234-4234-8234-123456789abc",
    fieldName: "title",
    expiresAt: "2026-11-16T00:00:00Z",
    status: "eligible",
  };
  const save = {
    updateMetadata: {
      requestId: "32345678-1234-4234-8234-123456789abc",
      operations: [operation],
      metadata: {
        manifestationId: id,
        workId: btoa("Work:42345678-1234-4234-8234-123456789abc"),
        etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
        title: "Saved",
      },
    },
  };
  fetch.mockResolvedValueOnce(json({ data: save }));
  await expect(
    graphqlNetwork(
      getRequest(mutationsSaveTitleMutation).params,
      {},
      new AbortController().signal,
    ).toPromise(),
  ).resolves.toEqual({ data: save });
  for (const bad of [
    { ...operation, operationId: "bad" },
    { ...operation, expiresAt: "yesterday" },
  ]) {
    fetch.mockClear();
    fetch.mockResolvedValueOnce(
      json({ data: { updateMetadata: { ...save.updateMetadata, operations: [bad] } } }),
    );
    const next = vi.fn();
    await expect(
      graphqlNetwork(
        getRequest(mutationsSaveTitleMutation).params,
        {},
        new AbortController().signal,
      )
        .do({ next })
        .toPromise(),
    ).rejects.toThrow();
    expect(next).not.toHaveBeenCalled();
    expect(fetch).toHaveBeenCalledTimes(1);
  }
  const navigate = vi.fn();
  setUnauthenticatedHandler(navigate);
  fetch.mockResolvedValueOnce(json({}, 401)).mockResolvedValueOnce(json({}, 401));
  const firstFailure = graphqlNetwork(
    getRequest(viewerQuery).params,
    {},
    new AbortController().signal,
  ).toPromise();
  const secondFailure = graphqlNetwork(
    getRequest(viewerQuery).params,
    {},
    new AbortController().signal,
  ).toPromise();
  await expect(firstFailure).rejects.toBeInstanceOf(ApiError);
  await expect(secondFailure).rejects.toBeInstanceOf(ApiError);
  expect(navigate).toHaveBeenCalledTimes(1);
  expect(activeUserId()).toBeNull();
  rememberActiveUser("second-account");
  expect(relayEnvironment().getStore().getSource().get("User:first-account")).toBeUndefined();
  fetch.mockResolvedValueOnce(
    json({ errors: [{ message: "Forbidden", extensions: { code: "forbidden" } }] }),
  );
  await expect(request()).rejects.toBeInstanceOf(GraphQLExecutionError);
  expect(activeUserId()).toBe("second-account");
  expect(navigate).toHaveBeenCalledTimes(1);
  const previous = relayEnvironment();
  let respond: (response: Response) => void = () => {
    throw new Error("Deferred request not initialised");
  };
  const pending = new Promise<Response>((resolve) => {
    respond = resolve;
  });
  fetch.mockImplementationOnce(() => pending);
  const published = vi.fn();
  const subscription = fetchQuery<ViewerQuery>(
    previous,
    viewerQuery,
    {},
    { fetchPolicy: "network-only" },
  ).subscribe({ next: published, error: published });
  const init = fetch.mock.calls.at(-1)?.[1];
  const epoch = activeUserEpoch();
  rememberActiveUser("third-account");
  expect(init?.signal?.aborted).toBe(true);
  respond(
    json({
      data: {
        viewer: { id: "User:second-account", displayName: "Old private data", isChild: false },
      },
    }),
  );
  fetch.mockResolvedValueOnce(
    json({
      data: { viewer: { id: "User:third-account", displayName: "Third reader", isChild: false } },
    }),
  );
  await request();
  expect(published).not.toHaveBeenCalled();
  expect(previous.getStore().getSource().get("User:second-account")).toBeUndefined();
  expect(relayEnvironment().getStore().getSource().get("User:second-account")).toBeUndefined();
  rememberActiveUser("second-account", epoch);
  expect(activeUserId()).toBe("third-account");
  subscription.unsubscribe();
});
