import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "./csrf";
import { getPreferences, updatePreferences } from "./preferences";

function parseJsonBody(body: BodyInit | null | undefined): unknown {
  if (typeof body !== "string") throw new Error("expected stringified JSON body");
  return JSON.parse(body);
}

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

const DEFAULTS = {
  hidden_columns: [],
  density: "comfortable",
  view: "grid",
  sort_stack: "-created_at",
};

const INHERITED = {
  hidden_columns: null,
  density: null,
  view: null,
  sort_stack: null,
  defaults: DEFAULTS,
};

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("getPreferences", () => {
  test("reads /auth/me/preferences and returns all-null overrides beside the defaults", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(INHERITED));

    const result = await getPreferences();

    expect(fetchSpy.mock.calls[0]?.[0]).toBe("/auth/me/preferences");
    expect(fetchSpy.mock.calls[0]?.[1]?.method).toBe("GET");
    expect(result).toEqual(INHERITED);
  });

  test("returns overrides distinct from the defaults they shadow", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({
        hidden_columns: ["subtitle", "isbn_13"],
        density: "compact",
        view: "table",
        sort_stack: "title,-pages",
        defaults: DEFAULTS,
      }),
    );

    const result = await getPreferences();

    expect(result.density).toBe("compact");
    expect(result.defaults.density).toBe("comfortable");
    expect(result.hidden_columns).toEqual(["subtitle", "isbn_13"]);
  });

  test("throws when a group carries a value outside the wire vocabulary", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ ...INHERITED, density: "roomy" }),
    );

    await expect(getPreferences()).rejects.toThrow();
  });

  test("throws when the defaults object is missing", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      jsonResponse({ hidden_columns: null, density: null, view: null, sort_stack: null }),
    );

    await expect(getPreferences()).rejects.toThrow();
  });
});

describe("updatePreferences", () => {
  test("PATCHes a bare merge-patch body carrying only the named group", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ...INHERITED, density: "compact" }));

    await updatePreferences({ density: "compact" });

    const [input, init] = fetchSpy.mock.calls[0] ?? [];
    expect(input).toBe("/auth/me/preferences");
    expect(init?.method).toBe("PATCH");
    expect(parseJsonBody(init?.body)).toEqual({ density: "compact" });
  });

  test("carries the CSRF token the session-authenticated PATCH requires", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ...INHERITED, density: "compact" }));

    await updatePreferences({ density: "compact" });

    const headers = new Headers(fetchSpy.mock.calls[0]?.[1]?.headers);
    expect(headers.get("X-CSRF-Token")).toBe("test-csrf-token-0000000000000000000000000");
  });

  test("an explicit null reaches the wire as a reset, not as an omission", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(INHERITED));

    await updatePreferences({ hidden_columns: null });

    expect(parseJsonBody(fetchSpy.mock.calls[0]?.[1]?.body)).toEqual({ hidden_columns: null });
  });

  test("an undefined group is omitted so the server leaves it unchanged", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse({ ...INHERITED, view: "table" }));

    await updatePreferences({ view: "table", density: undefined });

    expect(parseJsonBody(fetchSpy.mock.calls[0]?.[1]?.body)).toEqual({ view: "table" });
  });

  test("returns the parsed post-merge state", async () => {
    const merged = { ...INHERITED, hidden_columns: ["pages"], density: "compact" };
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(jsonResponse(merged));

    await expect(updatePreferences({ density: "compact" })).resolves.toEqual(merged);
  });

  test("surfaces a 422 rejection as an ApiError with the detail preserved", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValueOnce(
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/validation",
          title: "Unprocessable Entity",
          status: 422,
          detail: "sort_stack: unknown sort field",
        }),
        { status: 422, headers: { "Content-Type": "application/problem+json" } },
      ),
    );

    await expect(updatePreferences({ sort_stack: "nope" })).rejects.toMatchObject({
      status: 422,
      detail: "sort_stack: unknown sort field",
    });
  });
});
