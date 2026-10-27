import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import type { ReactElement } from "react";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { z } from "zod";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError } from "@/api/errors";
import { __resetEtagCacheForTesting } from "@/api/etags";
import {
  invokeUnauthenticatedHandler,
  queryClient as appQueryClient,
  setUnauthenticatedHandler,
} from "@/lib/query/client";

import { adminSettingsRoute } from "@/routes/production";
import { queryKeys } from "@/lib/query/keys";

import { SettingsPage } from "@/pages/admin/settings/SettingsPage";

const supported = vi.hoisted(() => ({ formats: ["epub"] as string[] }));

vi.mock("@/pages/admin/settings/fields", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  SUPPORTED_FORMATS: supported.formats,
}));

const BodySchema = z.record(z.string(), z.unknown());

type Values = Record<string, unknown>;

function urlOf(input: RequestInfo | URL): URL {
  let raw: string;
  if (typeof input === "string") {
    raw = input;
  } else if (input instanceof URL) {
    raw = input.href;
  } else {
    raw = input.url;
  }
  return new URL(raw, "http://localhost");
}

const BASE_VALUES: Values = {
  ingestion_enabled: true,
  accepted_formats: ["epub"],
  cleanup_imported: true,
  cleanup_duplicates: false,
  enrichment_enabled: true,
  enrichment_concurrency: 4,
  enrichment_poll_idle_secs: 30,
  enrichment_fetch_budget_secs: 20,
  cover_max_bytes: 10 * 1_048_576,
  cover_download_timeout_secs: 15,
  cover_min_long_edge_px: 600,
  cover_redirect_limit: 3,
  writeback_enabled: true,
  writeback_concurrency: 2,
  writeback_poll_idle_secs: 30,
  writeback_max_attempts: 5,
  opds_enabled: true,
  opds_page_size: 50,
  provider_visibility: {},
};

type PutRecord = { patch: Values; ifMatch: string | null };

type PutMode =
  | "ok"
  | "403"
  | "422"
  | "422-switch"
  | "422-formats"
  | "422-unmapped"
  | "network"
  | "500"
  | "401"
  | "428-first";

type Server = {
  role: "admin" | "adult";
  values: Values;
  revision: number;
  reload: string | null;
  getStatus: number;
  reloadStatus: number;
  putMode: PutMode;
  /** Revisions at which a PUT is answered 412 (the PUT count, 1-based), with a hook to move the data first. */
  stale: Map<number, () => void>;
  puts: PutRecord[];
  gets: number;
  getGate: Promise<void> | null;
};

let server: Server;

function newServer(over: Partial<Server> = {}): Server {
  return {
    role: "admin",
    values: { ...BASE_VALUES },
    revision: 1,
    reload: "2026-10-09T10:42:00Z",
    getStatus: 200,
    reloadStatus: 200,
    putMode: "ok",
    stale: new Map(),
    puts: [],
    gets: 0,
    getGate: null,
    ...over,
  };
}

const etag = (): string => `"rev-${String(server.revision)}"`;

function reply(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": status >= 400 ? "application/problem+json" : "application/json",
      ...headers,
    },
  });
}

function mismatch(): Response {
  return reply(
    {
      type: "https://reverie.example/probs/if-match-mismatch",
      title: "Precondition Failed",
      status: 412,
    },
    412,
    { ETag: etag() },
  );
}

function row(): Values {
  return { ...server.values, revision: server.revision, updated_at: "2026-10-09T10:31:00Z" };
}

function handlePut(init: RequestInit | undefined): Response {
  const patch = BodySchema.parse(JSON.parse(typeof init?.body === "string" ? init.body : "{}"));
  const sent = new Headers(init?.headers).get("If-Match");
  server.puts.push({ patch, ifMatch: sent });
  const attempt = server.puts.length;
  if (server.putMode === "403") return reply({ title: "Forbidden", status: 403 }, 403);
  if (server.putMode === "401") return reply({ title: "Unauthorized", status: 401 }, 401);
  if (server.putMode === "500") return reply({ title: "Internal Server Error", status: 500 }, 500);
  if (server.putMode === "network") throw new TypeError("Failed to fetch");
  if (server.putMode === "422") {
    return reply(
      {
        title: "Unprocessable",
        status: 422,
        detail: "enrichment_concurrency must be between 1 and 10",
      },
      422,
    );
  }
  if (server.putMode === "422-switch" || server.putMode === "422-formats") {
    const detail =
      server.putMode === "422-switch"
        ? "enrichment_enabled is not accepted"
        : "accepted_formats supports only epub";
    return reply({ title: "Unprocessable", status: 422, detail }, 422);
  }
  if (server.putMode === "422-unmapped") {
    return reply(
      { title: "Unprocessable", status: 422, detail: "request body is not acceptable" },
      422,
    );
  }
  if (server.putMode === "428-first" && attempt === 1) {
    return reply(
      {
        type: "https://reverie.example/probs/if-match-required",
        title: "Precondition Required",
        status: 428,
      },
      428,
    );
  }
  const hook = server.stale.get(attempt);
  if (hook !== undefined) {
    hook();
    return mismatch();
  }
  if (sent !== etag()) return mismatch();
  Object.assign(server.values, patch);
  server.revision += 1;
  return reply({ ...row(), restart_required: false }, 200, { ETag: etag() });
}

function installFetch(): void {
  vi.spyOn(globalThis, "fetch").mockImplementation((input, init) => {
    const url = urlOf(input);
    const method = (init?.method ?? "GET").toUpperCase();
    if (url.pathname === "/auth/me") {
      return Promise.resolve(
        reply({
          id: "11111111-1111-4111-8111-111111111111",
          display_name: "Ada Lovelace",
          email: "ada@example.org",
          role: server.role,
          is_child: false,
          has_local_password: true,
          theme_preference: "system",
          csrf_token: null,
        }),
      );
    }
    if (url.pathname === "/api/v1/settings/reload-status") {
      if (server.reloadStatus !== 200)
        return Promise.resolve(
          reply({ title: "Error", status: server.reloadStatus }, server.reloadStatus),
        );
      return Promise.resolve(reply({ last_successful_reload_at: server.reload }));
    }
    if (url.pathname === "/api/v1/settings" && method === "GET") {
      server.gets += 1;
      if (server.getStatus !== 200) {
        return Promise.resolve(
          reply({ title: "Error", status: server.getStatus }, server.getStatus),
        );
      }
      const answer = (): Response =>
        reply({ ...row(), restart_required_fields: [] }, 200, { ETag: etag() });
      return server.getGate === null ? Promise.resolve(answer()) : server.getGate.then(answer);
    }
    if (url.pathname === "/api/v1/settings" && method === "PUT") {
      try {
        return Promise.resolve(handlePut(init));
      } catch (err) {
        return Promise.reject(err instanceof Error ? err : new Error("failed"));
      }
    }
    return Promise.resolve(reply({ title: "Not Found", status: 404 }, 404));
  });
}

function newClient(): QueryClient {
  return new QueryClient({
    queryCache: new QueryCache({
      onError: (err) => {
        if (err instanceof ApiError && err.status === 401) invokeUnauthenticatedHandler();
      },
    }),
    defaultOptions: { queries: { retry: false } },
  });
}

async function renderSettings(
  area = "enrichment",
  client: QueryClient = newClient(),
  route: RouteObject = { path: "/admin/settings/:area", element: <SettingsPage /> },
): Promise<{
  router: ReturnType<typeof createMemoryRouter>;
  client: QueryClient;
}> {
  const router = createMemoryRouter(
    [route, { path: "/library", element: <div>Library page</div> }],
    { initialEntries: [`/admin/settings/${area}`] },
  );
  function Harness(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  await render(<Harness />);
  return { router, client };
}

async function loaded(area = "enrichment"): Promise<Awaited<ReturnType<typeof renderSettings>>> {
  const rendered = await renderSettings(area);
  await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
  return rendered;
}

async function type(user: typeof userEvent, label: string, text: string): Promise<void> {
  const input = page.getByRole("textbox", { name: label });
  await input.fill("");
  await user.type(input, text);
}

const save = (): Locator => page.getByRole("button", { name: /^Save/ });

function invalidControls(): (HTMLElement | SVGElement)[] {
  return [...page.getByRole("textbox").elements(), ...page.getByRole("switch").elements()].filter(
    (control) => control.getAttribute("aria-invalid") === "true",
  );
}

function holdPut(): () => void {
  let release: () => void = () => undefined;
  const gate = new Promise<void>((resolve) => {
    release = resolve;
  });
  const realFetch = vi.mocked(globalThis.fetch).getMockImplementation();
  vi.mocked(globalThis.fetch).mockImplementation(async (input, init) => {
    if ((init?.method ?? "GET") === "PUT") await gate;
    return realFetch === undefined ? new Response() : realFetch(input, init);
  });
  return release;
}

beforeEach(() => {
  server = newServer();
  __resetEtagCacheForTesting();
  installFetch();
});

afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
  setUnauthenticatedHandler(() => {});
});

describe("production settings navigation", () => {
  beforeEach(() => {
    appQueryClient.clear();
  });

  afterEach(() => {
    appQueryClient.clear();
  });

  test("a previous area's pending reconciliation cannot rebase the current dirty draft", async () => {
    const user = userEvent;
    await renderSettings("acquisition", appQueryClient, adminSettingsRoute);
    await expect
      .element(page.getByRole("form", { name: "Acquisition settings" }))
      .toBeInTheDocument();
    await user.click(
      page.elementLocator(
        (
          await rendered(
            page.getByRole("switch", { name: "Remove the source file after a successful import" }),
          )
        ).closest("label") ??
          (await rendered(
            page.getByRole("switch", { name: "Remove the source file after a successful import" }),
          )),
      ),
    );
    const release = holdPut();
    await user.click(save());
    await user.click(
      page.getByRole("navigation", { name: "Settings areas" }).getByRole("link", {
        name: /^Enrichment/,
      }),
    );
    await user.click(page.getByRole("button", { name: "Leave and discard changes" }));
    await expect
      .element(page.getByRole("form", { name: "Enrichment settings" }))
      .toBeInTheDocument();
    await type(user, "Concurrent lookups", "6");
    server.values.enrichment_concurrency = 5;
    server.revision += 1;

    release();
    await vi.waitFor(() => {
      expect(server.values.cleanup_imported).toBe(false);
    });
    expect(server.puts).toHaveLength(2);
    await expect.element(page.getByText("Saved value: 4 lookups")).toBeInTheDocument();
    await user.click(save());

    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    expect(server.puts).toHaveLength(3);
    expect(server.puts[2]?.ifMatch).toBe('"rev-1"');
    expect(server.values.enrichment_concurrency).toBe(5);
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
    await user.click(page.getByRole("button", { name: "Save my choices" }));
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(4);
    expect(server.puts[3]?.ifMatch).toBe('"rev-3"');
    expect(server.values.enrichment_concurrency).toBe(6);
  });

  test("a fresh visit refused with 403 hides a previously cached settings snapshot", async () => {
    const { router } = await renderSettings("enrichment", appQueryClient, adminSettingsRoute);
    await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
    await router.navigate("/library");
    await expect.element(page.getByText("Library page")).toBeInTheDocument();
    server.getStatus = 403;

    await router.navigate("/admin/settings/enrichment");

    await expect.element(page.getByText("Settings are for administrators")).toBeInTheDocument();
    await expect.element(page.getByRole("form")).not.toBeInTheDocument();
    expect(server.gets).toBeGreaterThan(1);
    expect(server.puts).toHaveLength(0);
  });

  test("revisiting the active area preserves the draft revision and detects a concurrent edit", async () => {
    const user = userEvent;
    const { router } = await renderSettings("enrichment", appQueryClient, adminSettingsRoute);
    await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
    await type(user, "Concurrent lookups", "6");
    server.values.enrichment_concurrency = 5;
    server.revision += 1;

    const nav = page.getByRole("navigation", { name: "Settings areas" });
    await user.click(nav.getByRole("link", { name: /^Enrichment/ }));
    await vi.waitFor(() => {
      expect(router.state.navigation.state).toBe("idle");
    });
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
    await user.click(save());
    await vi.waitFor(() => {
      expect(server.puts).toHaveLength(1);
    });
    expect(server.puts[0]?.ifMatch).toBe('"rev-1"');
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    expect(server.values.enrichment_concurrency).toBe(5);
    expect(server.puts).toHaveLength(1);
  });

  test.each(["invalid JSON", "invalid schema"])(
    "a failed re-entry read with %s cannot authorise a stale save",
    async (failure) => {
      const user = userEvent;
      const { router } = await renderSettings("enrichment", appQueryClient, adminSettingsRoute);
      await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
      await router.navigate("/library");
      await expect.element(page.getByText("Library page")).toBeInTheDocument();
      server.values.enrichment_concurrency = 5;
      server.revision += 1;
      const realFetch = vi.mocked(globalThis.fetch).getMockImplementation();
      vi.mocked(globalThis.fetch).mockImplementation((input, init) => {
        if (urlOf(input).pathname === "/api/v1/settings" && (init?.method ?? "GET") === "GET") {
          return Promise.resolve(
            new Response(failure === "invalid JSON" ? "{" : "{}", {
              headers: { "Content-Type": "application/json", ETag: etag() },
            }),
          );
        }
        return realFetch === undefined ? Promise.resolve(new Response()) : realFetch(input, init);
      });
      await router.navigate("/admin/settings/enrichment");
      await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
      installFetch();
      await type(user, "Concurrent lookups", "6");
      await user.click(save());
      await vi.waitFor(() => {
        expect(server.puts).toHaveLength(1);
      });
      expect(server.puts[0]?.ifMatch).toBe('"rev-1"');
      expect(server.values.enrichment_concurrency).toBe(5);
      await expect
        .element(page.getByText("Settings changed while you were editing"))
        .toBeInTheDocument();
    },
  );

  test("leaving a dirty area still confirms discard and loads fresh settings for the next area", async () => {
    const user = userEvent;
    await renderSettings("enrichment", appQueryClient, adminSettingsRoute);
    await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
    await type(user, "Concurrent lookups", "6");
    server.values.enrichment_concurrency = 5;
    server.revision += 1;

    const nav = page.getByRole("navigation", { name: "Settings areas" });
    await user.click(nav.getByRole("link", { name: /^Writeback/ }));
    await expect.element(page.getByRole("alertdialog")).toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Leave and discard changes" }));
    await expect
      .element(page.getByRole("form", { name: "Writeback settings" }))
      .toBeInTheDocument();
    await user.click(page.getByRole("link", { name: /^Enrichment/ }));
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("5");
    expect(server.puts).toHaveLength(0);
    expect(server.gets).toBe(3);
  });
});

describe("access", () => {
  test("a non-admin sees the administrators notice and no settings request is made", async () => {
    server = newServer({ role: "adult" });
    await renderSettings();
    await expect.element(page.getByText("Settings are for administrators")).toBeInTheDocument();
    await expect
      .element(page.getByRole("link", { name: "Back to the library" }))
      .toHaveAttribute("href", "/library");
    await expect.element(page.getByRole("form")).not.toBeInTheDocument();
    expect(server.gets).toBe(0);
  });

  test("a 403 on the settings read shows the same notice for an admin whose role lapsed", async () => {
    server = newServer({ getStatus: 403 });
    await renderSettings();
    await expect.element(page.getByText("Settings are for administrators")).toBeInTheDocument();
  });

  test("an unknown or unshipped area redirects to the first area", async () => {
    const { router } = await renderSettings("providers");
    await expect
      .element(page.getByRole("form", { name: "Acquisition settings" }))
      .toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/admin/settings/acquisition");
  });

  test("a 401 on load reaches the sign-in handler through the application's query client", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);
    server = newServer({ getStatus: 401 });
    appQueryClient.clear();
    await renderSettings("enrichment", appQueryClient);
    await vi.waitFor(() => {
      expect(handler).toHaveBeenCalledTimes(1);
    });
    appQueryClient.clear();
  });

  test("a 401 on save goes through the global sign-in handler and keeps the edits", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);
    server.putMode = "401";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await vi.waitFor(() => {
      expect(handler).toHaveBeenCalledTimes(1);
    });
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
  });
});

describe("loading", () => {
  test("shows the page head and a busy loading region before the data arrives", async () => {
    await renderSettings();
    await expect
      .element(page.getByRole("heading", { level: 1, name: "Settings" }))
      .toBeInTheDocument();
    await expect.element(page.getByText("Loading settings")).toBeInTheDocument();
    await expect
      .element(document.querySelector<HTMLElement>('[role="status"][aria-busy="true"]'))
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("form", { name: "Enrichment settings" }))
      .toBeInTheDocument();
  });

  test("the loading skeletons and the switch thumb opt out of motion under reduced motion", async () => {
    await renderSettings();
    await expect.element(page.getByText("Loading settings")).toBeInTheDocument();
    const skeletons = document.querySelectorAll<HTMLElement>("[data-slot=skeleton]");
    expect(skeletons.length).toBeGreaterThan(0);
    for (const skeleton of skeletons) {
      await expect.element(skeleton).toHaveClass("motion-reduce:animate-none");
    }
    await expect
      .element(page.getByRole("form", { name: "Enrichment settings" }))
      .toBeInTheDocument();
    await expect
      .element(
        (
          await rendered(page.getByRole("switch", { name: "Fetch metadata from external sources" }))
        ).closest("label"),
      )
      .toHaveClass("motion-reduce:[&>[data-slot=switch-thumb]]:transition-none");
  });

  test("a failed load shows a recoverable error and Try again fetches again", async () => {
    server = newServer({ getStatus: 503 });
    const user = userEvent;
    await renderSettings();
    await expect.element(page.getByText("Settings could not be loaded")).toBeInTheDocument();
    await expect.element(page.getByText(/Nothing has been changed/)).toBeInTheDocument();
    server.getStatus = 200;
    await user.click(page.getByRole("button", { name: "Try again" }));
    await expect
      .element(page.getByRole("form", { name: "Enrichment settings" }))
      .toBeInTheDocument();
  });

  test("renders the area's fields, the live sub-navigation summaries and the status line", async () => {
    await loaded();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("4");
    await expect
      .element(page.getByRole("switch", { name: "Fetch metadata from external sources" }))
      .toBeChecked();
    const nav = page.getByRole("navigation", { name: "Settings areas" });
    await expect
      .element(nav.getByRole("link", { name: /Enrichment/ }))
      .toHaveAttribute("aria-current", "page");
    await expect.element(nav.getByText("On. 4 lookups at a time")).toBeInTheDocument();
    await expect.element(nav.getByText("Up to 10 MiB")).toBeInTheDocument();
    await expect.element(nav.getByText(/Provider display/)).not.toBeInTheDocument();
    await expect.element(page.getByText("Last changed")).toBeInTheDocument();
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Discard changes" })).toBeDisabled();
    await expect.element(save()).toBeDisabled();
  });

  test("reload age uses the time the server status was read", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    try {
      vi.setSystemTime(new Date("2026-10-09T10:46:00Z"));
      await loaded();
      await expect.element(page.getByText(/4 minutes ago/)).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  test("a null reload time reads as the normal never-refreshed state", async () => {
    server = newServer({ reload: null });
    await loaded();
    await expect
      .element(page.getByText(/Not refreshed since the server started/))
      .toBeInTheDocument();
  });

  test("a failing reload-status read leaves the settings usable and omits that line", async () => {
    server = newServer({ reloadStatus: 500 });
    await loaded();
    await expect.element(page.getByText("Last changed")).toBeInTheDocument();
    await expect.element(page.getByText("Server cache refreshed")).not.toBeInTheDocument();
  });

  test("the MiB field shows the exact byte count beneath it", async () => {
    await loaded("covers");
    await expect.element(page.getByText("10,485,760 bytes")).toBeInTheDocument();
    const user = userEvent;
    await type(user, "Largest cover file", "0.5");
    await expect.element(page.getByText("524,288 bytes")).toBeInTheDocument();
  });
});

describe("editing", () => {
  test("an edit marks the row, the sub-navigation item and the bar; Discard reverts it", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await expect.element(page.getByText("Saved value: 4 lookups")).toBeInTheDocument();
    await expect.element(page.getByText("1 unsaved change")).toBeInTheDocument();
    const nav = page.getByRole("navigation", { name: "Settings areas" });
    await expect.element(nav.getByText("Edited")).toBeInTheDocument();
    await type(user, "Time allowed per source", "30");
    await expect.element(page.getByText("2 unsaved changes")).toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Discard changes" }));
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("4");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
    await expect.element(page.getByText("Edited")).not.toBeInTheDocument();
  });

  test("Discard changes becomes disabled after discarding and is skipped by Tab", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    const discard = page.getByRole("button", { name: "Discard changes" });
    {
      (await rendered(discard)).focus();
    }
    await user.keyboard("{Enter}");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
    await expect.element(discard).toBeDisabled();
    await user.tab();
    await expect.element(discard).not.toHaveFocus();
    await expect.element(save()).not.toHaveFocus();
    expect(server.puts).toHaveLength(0);
  });

  test("editing back to the saved value clears the dirty state", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await type(user, "Concurrent lookups", "4");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
    await expect.element(save()).toBeDisabled();
  });

  test("an area's edits never appear in another area's bar", async () => {
    const user = userEvent;
    const { router } = await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await user.click(page.getByRole("button", { name: "Leave and discard changes" }));
    await expect.element(page.getByRole("form", { name: "Covers settings" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/admin/settings/covers");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });
});

describe("import admission", () => {
  test("is the first switch, keeps edits local, discards and saves through the existing form", async () => {
    const user = userEvent;
    const { client } = await loaded("acquisition");
    const status = { blockers: [] };
    client.setQueryData(queryKeys.ingestion.status(), status);
    await expect
      .element(page.getByRole("switch").elements()[0])
      .toHaveAccessibleName("Import files");
    const control = page.getByRole("switch", { name: "Import files" });
    await expect.element(control).toBeChecked();
    await expect
      .element(control)
      .toHaveAccessibleDescription(/The setting takes effect when you save/);
    await user.click(
      page.elementLocator((await rendered(control)).closest("label") ?? (await rendered(control))),
    );
    expect(server.puts).toHaveLength(0);
    await expect
      .element(page.getByRole("link", { name: /Acquisition/ }))
      .toMatchTextContent("Imports on.");
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    await expect.element(page.getByRole("switch", { name: /successful import/ })).toBeEnabled();
    await user.click(page.getByRole("button", { name: "Discard changes" }));
    await expect.element(control).toBeChecked();
    await user.click(
      page.elementLocator((await rendered(control)).closest("label") ?? (await rendered(control))),
    );
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toEqual([{ patch: { ingestion_enabled: false }, ifMatch: '"rev-1"' }]);
    await expect
      .element(page.getByRole("link", { name: /Acquisition/ }))
      .toMatchTextContent("Imports off.");
    expect(client.getQueryState(queryKeys.ingestion.status())?.isInvalidated).toBe(true);
    expect(client.getQueryData(queryKeys.ingestion.status())).toEqual(status);
  });

  test("a failed save retains the unsaved switch and does not invalidate status", async () => {
    const user = userEvent;
    server.putMode = "network";
    const { client } = await loaded("acquisition");
    client.setQueryData(queryKeys.ingestion.status(), { blockers: [] });
    await user.click(
      page.elementLocator(
        (await rendered(page.getByRole("switch", { name: "Import files" }))).closest("label") ??
          (await rendered(page.getByRole("switch", { name: "Import files" }))),
      ),
    );
    await user.click(save());
    await vi.waitFor(() => {
      expect(server.puts).toHaveLength(1);
    });
    await vi.waitFor(async () => {
      await expect.element(save()).not.toMatchTextContent("Saving");
    });
    await expect.element(page.getByRole("switch", { name: "Import files" })).not.toBeChecked();
    await expect
      .element(page.getByRole("link", { name: /Acquisition/ }))
      .toMatchTextContent("Imports on.");
    expect(client.getQueryState(queryKeys.ingestion.status())?.isInvalidated).toBe(false);
  });

  test("a stale save recognises an already saved disable and invalidates status", async () => {
    const user = userEvent;
    const { client } = await loaded("acquisition");
    client.setQueryData(queryKeys.ingestion.status(), { blockers: [] });
    server.stale.set(1, () => {
      server.values.ingestion_enabled = false;
      server.revision += 1;
    });
    await user.click(
      page.elementLocator(
        (await rendered(page.getByRole("switch", { name: "Import files" }))).closest("label") ??
          (await rendered(page.getByRole("switch", { name: "Import files" }))),
      ),
    );
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await expect.element(page.getByRole("switch", { name: "Import files" })).not.toBeChecked();
    expect(server.puts).toHaveLength(1);
    expect(client.getQueryState(queryKeys.ingestion.status())?.isInvalidated).toBe(true);
  });
});

describe("accepted formats", () => {
  test("one supported format is a read-only line with no control and no save payload", async () => {
    const user = userEvent;
    await loaded("acquisition");
    const row = page.getByRole("form", { name: "Acquisition settings" });
    await expect.element(row.getByText("Reverie imports EPUB files.")).toBeInTheDocument();
    await expect.element(row.getByRole("checkbox", { name: "EPUB" })).not.toBeInTheDocument();
    await expect
      .element(row.getByText(/is the only format Reverie supports/))
      .not.toBeInTheDocument();
    expect(document.getElementById("help-accepted_formats")).toBeNull();
    await expect
      .element(document.getElementById("help-ingestion_enabled"))
      .toMatchTextContent("Reverie stops starting new imports");
    const field = document.querySelector('[data-field="accepted_formats"]');
    expect(field?.querySelector("input, button, a[href], [tabindex], [role=checkbox]")).toBeNull();
    await expect.element(page.getByText(/can't be turned off/)).not.toBeInTheDocument();
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
    await user.click(
      page.elementLocator(
        (
          await rendered(
            page.getByRole("switch", {
              name: "Remove the source file when the book is a duplicate",
            }),
          )
        ).closest("label") ??
          (await rendered(
            page.getByRole("switch", {
              name: "Remove the source file when the book is a duplicate",
            }),
          )),
      ),
    );
    await user.click(page.getByRole("button", { name: "Turn on removal" }));
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    expect(server.puts[0]?.patch).toEqual({ cleanup_duplicates: true });
    expect(server.puts[0]?.patch).not.toHaveProperty("accepted_formats");
  });

  describe("with two supported formats", () => {
    beforeEach(() => {
      supported.formats.splice(0, supported.formats.length, "epub", "pdf");
    });

    afterEach(() => {
      supported.formats.splice(0, supported.formats.length, "epub");
    });

    test("the checkbox list renders and the last enabled format is read-only", async () => {
      const user = userEvent;
      await loaded("acquisition");
      await expect.element(page.getByText("Reverie imports EPUB files.")).not.toBeInTheDocument();
      expect(document.getElementById("help-accepted_formats")).toBeNull();
      const epub = page.getByRole("checkbox", { name: "EPUB" });
      const pdf = page.getByRole("checkbox", { name: "PDF" });
      await expect.element(epub).toBeChecked();
      await expect.element(pdf).not.toBeChecked();
      await expect.element(epub).toHaveAttribute("aria-readonly", "true");
      await expect.element(epub).not.toHaveAttribute("disabled");
      await expect.element(pdf).not.toHaveAttribute("aria-readonly");
      await expect
        .element(epub)
        .toHaveAccessibleDescription(
          "This is the only format selected. At least one format must stay selected, or Reverie would have nothing to import.",
        );
      await user.click(
        page.elementLocator((await rendered(epub)).closest("label") ?? (await rendered(epub))),
      );
      await expect.element(epub).toBeChecked();
      {
        (await rendered(epub)).focus();
      }
      await user.keyboard(" ");
      await expect.element(epub).toBeChecked();
      await expect.element(epub).toHaveFocus();
      await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
      expect(server.puts).toEqual([]);
    });

    test("enabling a second format unlocks the first and saves the whole list", async () => {
      const user = userEvent;
      await loaded("acquisition");
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("checkbox", { name: "PDF" }))).closest("label") ??
            (await rendered(page.getByRole("checkbox", { name: "PDF" }))),
        ),
      );
      const epub = page.getByRole("checkbox", { name: "EPUB" });
      await expect.element(epub).not.toHaveAttribute("aria-readonly");
      await expect
        .element(page.getByText(/This is the only format selected/))
        .not.toBeInTheDocument();
      const release = holdPut();
      await user.click(save());
      await expect.element(epub).toBeDisabled();
      await expect.element(page.getByRole("checkbox", { name: "PDF" })).toBeDisabled();
      {
        release();
      }
      await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
      expect(server.puts[0]?.patch).toEqual({ accepted_formats: ["epub", "pdf"] });
      await user.click(
        page.elementLocator((await rendered(epub)).closest("label") ?? (await rendered(epub))),
      );
      await expect.element(epub).not.toBeChecked();
      await expect
        .element(page.getByRole("checkbox", { name: "PDF" }))
        .toHaveAttribute("aria-readonly", "true");
    });

    test("a mapped server 422 names the formats group and keeps the selection", async () => {
      server.putMode = "422-formats";
      const user = userEvent;
      await loaded("acquisition");
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("checkbox", { name: "PDF" }))).closest("label") ??
            (await rendered(page.getByRole("checkbox", { name: "PDF" }))),
        ),
      );
      await user.click(save());
      const message = page.getByText("accepted_formats supports only epub");
      await expect.element(message).toBeInTheDocument();
      await expect
        .element(page.getByRole("alert"))
        .toMatchTextContent("Settings were not savedReview the field errors and try again.");
      await expect.element(page.getByRole("checkbox", { name: "PDF" })).toBeChecked();
      await expect
        .element(page.getByRole("checkbox", { name: "PDF" }))
        .toHaveAccessibleDescription(/accepted_formats supports only epub/);
    });
  });
});

describe("saving", () => {
  test("sends only the dirty fields with the tag the read returned, then reports the save", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await type(user, "Time allowed per source", "30");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await expect
      .element(page.getByText("Changes are in effect now. No restart is needed."))
      .toBeInTheDocument();
    expect(server.puts).toEqual([
      {
        patch: { enrichment_concurrency: 6, enrichment_fetch_budget_secs: 30 },
        ifMatch: '"rev-1"',
      },
    ]);
    await expect.element(page.getByText(/^Saved at .*\d.*\.$/)).toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
    const nav = page.getByRole("navigation", { name: "Settings areas" });
    await expect.element(nav.getByText("On. 6 lookups at a time")).toBeInTheDocument();
    await expect.element(page.getByText("Edited")).not.toBeInTheDocument();
  });

  test("a second save carries the tag the first save returned", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await type(user, "Concurrent lookups", "7");
    await user.click(save());
    await vi.waitFor(() => {
      expect(server.puts).toHaveLength(2);
    });
    expect(server.puts[1]?.ifMatch).toBe('"rev-2"');
  });

  test("a read slow enough to land after another admin's save never rebases an active draft", async () => {
    const user = userEvent;
    await loaded();
    let release = (): void => {};
    server.getGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await type(user, "Concurrent lookups", "7");
    server.values.enrichment_concurrency = 8;
    server.revision += 1;
    server.getGate = null;
    release();
    await new Promise((resolve) => setTimeout(resolve, 50));
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    expect(server.puts[1]?.ifMatch).toBe('"rev-2"');
    expect(server.values.enrichment_concurrency).toBe(8);
  });

  test("an edit after a save clears the saved notice", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await type(user, "Concurrent lookups", "7");
    await expect.element(page.getByText("Settings saved")).not.toBeInTheDocument();
  });

  test("never sends an empty patch", async () => {
    const user = userEvent;
    await loaded();
    await user.click(save()).catch(() => undefined);
    expect(server.puts).toHaveLength(0);
  });

  test("controls are disabled and the button reads Saving while the request is in flight", async () => {
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const realFetch = vi.mocked(globalThis.fetch).getMockImplementation();
    vi.mocked(globalThis.fetch).mockImplementation(async (input, init) => {
      if ((init?.method ?? "GET") === "PUT") await gate;
      return realFetch === undefined ? new Response() : realFetch(input, init);
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    const busy = page.getByRole("button", { name: "Saving" });
    await expect.element(busy).toHaveAttribute("data-pending");
    await expect.element(page.getByRole("button", { name: "Discard changes" })).toBeDisabled();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveAttribute("readonly");
    await expect.element(page.getByText("Saving 1 change")).toBeInTheDocument();
    await user.click(busy, { force: true });
    release();
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
  });

  test("Save retains focus and suppresses repeat activation while pending, then becomes disabled", async () => {
    const release = holdPut();
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    {
      (await rendered(save())).focus();
    }
    await user.keyboard("{Enter}");
    const busy = page.getByRole("button", { name: "Saving" });
    await expect.element(busy).toHaveFocus();
    await expect.element(busy).toHaveAttribute("aria-disabled", "true");
    await expect.element(busy).not.toHaveAttribute("disabled");
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    await user.click(busy, { force: true });
    await expect.element(page.getByRole("status")).toMatchTextContent("Saving 1 change");
    release();
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    await expect.element(save()).toBeDisabled();
    await user.tab();
    await expect.element(save()).not.toHaveFocus();
    await user.keyboard("{Enter}");
    expect(server.puts).toHaveLength(1);
  });

  test("pressing Enter in a field keeps focus in that field while saving and after the save", async () => {
    const release = holdPut();
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await user.keyboard("{Enter}");
    await expect.element(page.getByText("Saving 1 change")).toBeInTheDocument();
    await expect.element(field).toHaveFocus();
    await user.keyboard("9");
    await expect.element(field).toHaveValue("6");
    release();
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await expect.element(field).toHaveFocus();
    await expect.element(field).not.toHaveAttribute("readonly");
    expect(server.puts).toHaveLength(1);
  });
});

describe("a single-text-field area", () => {
  test("repeated Enter and clicks while saving send one request", async () => {
    const release = holdPut();
    const user = userEvent;
    await loaded("catalogue");
    await type(user, "Entries per page", "60");
    await user.keyboard("{Enter}");
    await expect.element(page.getByText("Saving 1 change")).toBeInTheDocument();
    await user.keyboard("{Enter}");
    await user.keyboard("{Enter}");
    await user.click(page.getByRole("button", { name: "Saving" }), { force: true });
    release();
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    expect(server.puts[0]?.patch).toEqual({ opds_page_size: 60 });
  });
});

describe("validation", () => {
  test("native validation owns settings errors", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await type(user, "Idle check interval", "0");
    await user.click(save());
    const concurrency = page.getByRole("textbox", { name: "Concurrent lookups" });
    const idle = page.getByRole("textbox", { name: "Idle check interval" });
    await vi.waitFor(async () => {
      await expect
        .element(concurrency)
        .toHaveAccessibleDescription(/Enter a whole number from 1 to 10\./);
    });
    await expect.element(idle).toHaveAccessibleDescription(/Enter a whole number of 1 or more\./);
    await expect.element(concurrency).toHaveAttribute("aria-invalid", "true");
    await expect.element(idle).toHaveAttribute("aria-invalid", "true");
    await expect.element(concurrency).toHaveFocus();
    expect(server.puts).toHaveLength(0);
    await expect.element(page.getByText("Settings were not saved")).not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: /^Go to / })).not.toBeInTheDocument();
    await expect.element(page.getByText(/need attention/)).not.toBeInTheDocument();
  });

  test("an invalid submission keeps the drafts and the bar's unsaved count", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await type(user, "Idle check interval", "0");
    await user.click(save());
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("textbox", { name: "Concurrent lookups" })).toHaveFocus();
    });
    await expect.element(page.getByText("2 unsaved changes")).toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("12");
    await expect
      .element(page.getByRole("textbox", { name: "Idle check interval" }))
      .toHaveValue("0");
    await expect.element(save()).toBeEnabled();
  });

  test("correcting an invalid field clears its error and then saves", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await user.click(save());
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await vi.waitFor(async () => {
      await expect
        .element(field)
        .toHaveAccessibleDescription(/Enter a whole number from 1 to 10\.$/);
    });
    const message = page.getByText("Enter a whole number from 1 to 10.");
    expect((await rendered(message)).textContent).toBe("Enter a whole number from 1 to 10.");
    await expect
      .element((await rendered(message)).querySelector("svg"))
      .toHaveAttribute("aria-hidden", "true");
    await type(user, "Concurrent lookups", "8");
    await user.tab();
    await expect.element(field).not.toHaveAttribute("aria-invalid");
    await expect.element(field).not.toHaveAccessibleDescription(/Enter a whole number/);
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts.map((put) => put.patch)).toEqual([{ enrichment_concurrency: 8 }]);
  });

  test("Discard after an invalid submission clears the error and the next save sends only the new edit", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await user.click(save());
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await vi.waitFor(async () => {
      await expect.element(field).toHaveAttribute("aria-invalid", "true");
    });
    await user.click(page.getByRole("button", { name: "Discard changes" }));
    await expect.element(field).toHaveValue("4");
    await expect.element(field).not.toHaveAttribute("aria-invalid");
    await expect.element(field).not.toHaveAccessibleDescription(/Enter a whole number/);
    await type(user, "Idle check interval", "40");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts.map((put) => put.patch)).toEqual([{ enrichment_poll_idle_secs: 40 }]);
    await expect.element(field).toHaveValue("4");
    await expect.element(field).not.toHaveAttribute("aria-invalid");
  });

  test("an invalid edit after a successful save is blocked without a request and keeps the saved base", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await type(user, "Concurrent lookups", "12");
    await expect.element(page.getByText("Saved value: 6 lookups")).toBeInTheDocument();
    await user.click(save());
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await vi.waitFor(async () => {
      await expect.element(field).toHaveFocus();
    });
    await expect.element(field).toHaveAccessibleDescription(/Enter a whole number from 1 to 10\./);
    expect(server.puts).toHaveLength(1);
    await user.click(page.getByRole("button", { name: "Discard changes" }));
    await expect.element(field).toHaveValue("6");
    await expect.element(field).not.toHaveAttribute("aria-invalid");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("moving to another area and back starts without a displayed error", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await user.click(save());
    await vi.waitFor(async () => {
      await expect
        .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
        .toHaveAttribute("aria-invalid", "true");
    });
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await user.click(page.getByRole("button", { name: "Leave and discard changes" }));
    await expect.element(page.getByRole("form", { name: "Covers settings" })).toBeInTheDocument();
    expect(invalidControls()).toHaveLength(0);
    await user.click(page.getByRole("link", { name: /Enrichment/ }));
    await expect
      .element(page.getByRole("form", { name: "Enrichment settings" }))
      .toBeInTheDocument();
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await expect.element(field).toHaveValue("4");
    await expect.element(field).not.toHaveAttribute("aria-invalid");
  });

  test("discarding a deleted-source switch that was turned off opens no confirmation", async () => {
    const user = userEvent;
    await loaded("acquisition");
    const control = page.getByRole("switch", { name: /successful import/ });
    await expect.element(control).toBeChecked();
    await user.click(
      page.elementLocator((await rendered(control)).closest("label") ?? (await rendered(control))),
    );
    await expect.element(control).not.toBeChecked();
    await user.click(page.getByRole("button", { name: "Discard changes" }));
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    await expect.element(control).toBeChecked();
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("a general server failure invents no invalid field", async () => {
    server.putMode = "500";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("The server returned an error. Your edits are still here."))
      .toBeInTheDocument();
    expect(invalidControls()).toHaveLength(0);
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .not.toHaveAccessibleDescription(/server/);
  });

  test("non-numeric text reports Enter a number", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "many");
    await user.click(save());
    await expect.element(page.getByText("Enter a number.")).toBeInTheDocument();
  });

  test("a server 422 that names a field marks that field and announces the refusal without moving focus", async () => {
    server.putMode = "422";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await vi.waitFor(async () => {
      await expect.element(field).toHaveAttribute("aria-invalid", "true");
    });
    await expect
      .element(field)
      .toHaveAccessibleDescription(/enrichment_concurrency must be between 1 and 10/);
    await expect.element(field).toHaveValue("6");
    await expect
      .element(page.getByRole("alert"))
      .toMatchTextContent("Settings were not savedReview the field errors and try again.");
    await vi.waitFor(async () => {
      await expect.element(save()).toBeEnabled();
    });
    await expect.element(save()).toHaveFocus();
  });

  test("a mapped server 422 on a switch marks that switch without moving focus", async () => {
    server.putMode = "422-switch";
    const user = userEvent;
    await loaded();
    await user.click(
      page.elementLocator(
        (
          await rendered(page.getByRole("switch", { name: "Fetch metadata from external sources" }))
        ).closest("label") ??
          (await rendered(
            page.getByRole("switch", { name: "Fetch metadata from external sources" }),
          )),
      ),
    );
    await user.click(save());
    const control = page.getByRole("switch", { name: "Fetch metadata from external sources" });
    await vi.waitFor(async () => {
      await expect.element(control).toHaveAttribute("aria-invalid", "true");
    });
    await expect.element(control).toHaveAccessibleDescription(/enrichment_enabled is not accepted/);
    await expect
      .element(page.getByRole("alert"))
      .toMatchTextContent("Review the field errors and try again.");
    await vi.waitFor(async () => {
      await expect.element(save()).toBeEnabled();
    });
    await expect.element(save()).toHaveFocus();
    await expect.element(control).not.toBeChecked();
    expect(server.puts).toHaveLength(1);

    await user.click(save());
    expect(server.puts).toHaveLength(1);
    await expect.element(control).toHaveAttribute("aria-invalid", "true");
    await expect.element(control).toHaveFocus();
    await expect
      .element(page.getByRole("alert"))
      .toMatchTextContent("Review the field errors and try again.");
  });

  test("editing a field that carries a server error clears it and the next save goes through", async () => {
    server.putMode = "422";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    const field = page.getByRole("textbox", { name: "Concurrent lookups" });
    await vi.waitFor(async () => {
      await expect.element(field).toHaveAttribute("aria-invalid", "true");
    });
    server.putMode = "ok";
    await type(user, "Concurrent lookups", "7");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts.map((put) => put.patch)).toEqual([
      { enrichment_concurrency: 6 },
      { enrichment_concurrency: 7 },
    ]);
    await expect.element(field).not.toHaveAttribute("aria-invalid");
  });

  test("a server 422 that names no field shows the unattributed message", async () => {
    server.putMode = "422-unmapped";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(
        page.getByText("The server did not accept this change: request body is not acceptable."),
      )
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .not.toHaveAttribute("aria-invalid");
    expect(invalidControls()).toHaveLength(0);
  });

  test("a server 422 for the static one-format row stays a general alert", async () => {
    server.putMode = "422-formats";
    const user = userEvent;
    await loaded("acquisition");
    await user.click(
      page.elementLocator(
        (await rendered(page.getByRole("switch", { name: "Import files" }))).closest("label") ??
          (await rendered(page.getByRole("switch", { name: "Import files" }))),
      ),
    );
    await user.click(save());
    await expect
      .element(
        page.getByText(
          "The server did not accept this change: accepted_formats supports only epub.",
        ),
      )
      .toBeInTheDocument();
    expect(invalidControls()).toHaveLength(0);
  });
});

describe("save failures", () => {
  test.each([412, 428])("a 403 while reconciling %i keeps the draft", async (status) => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    server.getStatus = 403;
    if (status === 428) server.putMode = "428-first";
    else
      server.stale.set(1, () => {
        server.revision += 1;
      });

    await user.click(save());

    await expect.element(page.getByText("Nothing was saved")).toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
    expect(server.gets).toBe(2);
    expect(server.puts).toHaveLength(1);
    expect(server.values.enrichment_concurrency).toBe(4);
    await user.click(page.getByRole("link", { name: "Leave settings" }));
    await expect.element(page.getByText("Library page")).toBeInTheDocument();
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("a 403 keeps the edits, says nothing was saved and leaves without the discard prompt", async () => {
    server.putMode = "403";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Nothing was saved")).toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
    await user.click(page.getByRole("link", { name: "Leave settings" }));
    await expect.element(page.getByText("Library page")).toBeInTheDocument();
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("a network failure keeps the edits and says the server could not be reached", async () => {
    server.putMode = "network";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("The server could not be reached. Your edits are still here."))
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
    await expect.element(save()).toBeEnabled();
  });

  test("a server error keeps the edits and can be retried", async () => {
    server.putMode = "500";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("The server returned an error. Your edits are still here."))
      .toBeInTheDocument();
    server.putMode = "ok";
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
  });
});

describe("leaving with unsaved edits", () => {
  test("Stay keeps the page and the draft; focus lands on Stay first", async () => {
    const user = userEvent;
    const { router } = await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(page.getByRole("link", { name: /Covers/ }));
    const dialog = page.getByRole("alertdialog");
    await expect.element(dialog.getByText("Leave without saving?")).toBeInTheDocument();
    await expect
      .element(
        dialog.getByText(
          /You have 1 unsaved change in Enrichment\. It will be lost if you leave\./,
        ),
      )
      .toBeInTheDocument();
    await expect.element(dialog.getByRole("button", { name: "Stay on this page" })).toHaveFocus();
    await user.click(dialog.getByRole("button", { name: "Stay on this page" }));
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/admin/settings/enrichment");
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("6");
  });

  test("Escape means Stay", async () => {
    const user = userEvent;
    const { router } = await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await expect.element(page.getByRole("alertdialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    });
    expect(router.state.location.pathname).toBe("/admin/settings/enrichment");
  });

  test("Stay and Escape return focus to the control that opened the prompt", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    const covers = page.getByRole("link", { name: /Covers/ });
    await user.click(covers);
    await user.click(
      page.getByRole("alertdialog").getByRole("button", {
        name: "Stay on this page",
      }),
    );
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    });
    await vi.waitFor(async () => {
      await expect.element(covers).toHaveFocus();
    });
    await user.click(covers);
    await expect.element(page.getByRole("alertdialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    });
    await vi.waitFor(async () => {
      await expect.element(covers).toHaveFocus();
    });
  });

  test("Leave discards the draft; coming back shows the saved value", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await type(user, "Time allowed per source", "30");
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await expect
      .element(
        page.getByText(
          /You have 2 unsaved changes in Enrichment\. They will be lost if you leave\./,
        ),
      )
      .toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Leave and discard changes" }));
    await expect.element(page.getByRole("form", { name: "Covers settings" })).toBeInTheDocument();
    await user.click(page.getByRole("link", { name: /Enrichment/ }));
    await expect
      .element(page.getByRole("form", { name: "Enrichment settings" }))
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("4");
  });

  test("a pristine page navigates away with no prompt", async () => {
    const user = userEvent;
    await loaded();
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await expect.element(page.getByRole("form", { name: "Covers settings" })).toBeInTheDocument();
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("after a successful save the page is clean and navigation is not blocked", async () => {
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await expect.element(page.getByRole("form", { name: "Covers settings" })).toBeInTheDocument();
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("registers a beforeunload warning only while the page has unsaved edits", async () => {
    const add = vi.spyOn(window, "addEventListener");
    const user = userEvent;
    await loaded();
    expect(add.mock.calls.filter(([type]) => type === "beforeunload")).toHaveLength(0);
    await type(user, "Concurrent lookups", "6");
    expect(add.mock.calls.filter(([type]) => type === "beforeunload")).toHaveLength(1);
    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    await user.click(page.getByRole("button", { name: "Discard changes" }));
    const after = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(after);
    expect(after.defaultPrevented).toBe(false);
  });
});

describe("source-file deletion switches", () => {
  test("turning one on asks first; Keep leaves it off and returns focus to the switch", async () => {
    const user = userEvent;
    await loaded("acquisition");
    const toggle = page.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    {
      (await rendered(toggle)).focus();
    }
    await user.keyboard(" ");
    const dialog = page.getByRole("alertdialog");
    await expect
      .element(dialog.getByText("Remove source files for duplicates?"))
      .toBeInTheDocument();
    await expect
      .element(dialog.getByText(/This cannot be undone\. The setting takes effect when you save\./))
      .toBeInTheDocument();
    await vi.waitFor(async () => {
      await expect.element(dialog.getByRole("button", { name: "Keep source files" })).toHaveFocus();
    });
    await expect.element(toggle).not.toBeChecked();
    await user.click(dialog.getByRole("button", { name: "Keep source files" }));
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    });
    await expect.element(toggle).not.toBeChecked();
    await vi.waitFor(async () => {
      await expect.element(toggle).toHaveFocus();
    });
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("Escape behaves as Keep", async () => {
    const user = userEvent;
    await loaded("acquisition");
    const toggle = page.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(
      page.elementLocator((await rendered(toggle)).closest("label") ?? (await rendered(toggle))),
    );
    await expect.element(page.getByRole("alertdialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    });
    await expect.element(toggle).not.toBeChecked();
  });

  test("confirming turns it on, marks the row edited and states the consequence; nothing is saved yet", async () => {
    const user = userEvent;
    await loaded("acquisition");
    const toggle = page.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(
      page.elementLocator((await rendered(toggle)).closest("label") ?? (await rendered(toggle))),
    );
    await user.click(page.getByRole("button", { name: "Turn on removal" }));
    await vi.waitFor(async () => {
      await expect.element(toggle).toBeChecked();
    });
    await expect
      .element(
        page.getByText(
          /When you save, Reverie starts deleting the original file from the ingestion folder when the library already holds the same book\. This cannot be undone\./,
        ),
      )
      .toBeInTheDocument();
    await expect.element(page.getByText("1 unsaved change")).toBeInTheDocument();
    expect(server.puts).toHaveLength(0);
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts[0]?.patch).toEqual({ cleanup_duplicates: true });
  });

  test("the import switch has its own wording", async () => {
    server = newServer({ values: { ...BASE_VALUES, cleanup_imported: false } });
    const user = userEvent;
    await loaded("acquisition");
    await user.click(
      page.elementLocator(
        (
          await rendered(
            page.getByRole("switch", { name: "Remove the source file after a successful import" }),
          )
        ).closest("label") ??
          (await rendered(
            page.getByRole("switch", { name: "Remove the source file after a successful import" }),
          )),
      ),
    );
    await expect.element(page.getByText("Remove source files after import?")).toBeInTheDocument();
  });

  test("turning one off needs no confirmation", async () => {
    const user = userEvent;
    await loaded("acquisition");
    await user.click(
      page.elementLocator(
        (
          await rendered(
            page.getByRole("switch", { name: "Remove the source file after a successful import" }),
          )
        ).closest("label") ??
          (await rendered(
            page.getByRole("switch", { name: "Remove the source file after a successful import" }),
          )),
      ),
    );
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    await expect.element(page.getByText("1 unsaved change")).toBeInTheDocument();
  });

  test("turning one on and back off before saving leaves nothing dirty", async () => {
    const user = userEvent;
    await loaded("acquisition");
    const toggle = page.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(
      page.elementLocator((await rendered(toggle)).closest("label") ?? (await rendered(toggle))),
    );
    await user.click(page.getByRole("button", { name: "Turn on removal" }));
    await vi.waitFor(async () => {
      await expect.element(toggle).toBeChecked();
    });
    await user.click(
      page.elementLocator((await rendered(toggle)).closest("label") ?? (await rendered(toggle))),
    );
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("a switch that deletes nothing never asks", async () => {
    const user = userEvent;
    server = newServer({ values: { ...BASE_VALUES, enrichment_enabled: false } });
    await loaded();
    await user.click(
      page.elementLocator(
        (
          await rendered(page.getByRole("switch", { name: "Fetch metadata from external sources" }))
        ).closest("label") ??
          (await rendered(
            page.getByRole("switch", { name: "Fetch metadata from external sources" }),
          )),
      ),
    );
    await expect.element(page.getByRole("alertdialog")).not.toBeInTheDocument();
    await expect
      .element(page.getByRole("switch", { name: "Fetch metadata from external sources" }))
      .toBeChecked();
  });
});

describe("a stale write", () => {
  async function editTwoFields(user: typeof userEvent): Promise<void> {
    await type(user, "Concurrent lookups", "6");
    await type(user, "Time allowed per source", "30");
  }

  test("with no overlapping field it reloads, reapplies once, and says what the other admin changed", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_poll_idle_secs = 45;
      server.values.writeback_max_attempts = 3;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
    expect(server.puts[0]?.ifMatch).toBe('"rev-1"');
    expect(server.puts[1]).toEqual({
      patch: { enrichment_concurrency: 6, enrichment_fetch_budget_secs: 30 },
      ifMatch: '"rev-2"',
    });
    await expect
      .element(page.getByText(/None of your changes overlapped with theirs/))
      .toBeInTheDocument();
    await expect
      .element(
        page.getByText(
          "Also changed by someone else: Writeback, Attempts per file, now 3 attempts.",
        ),
      )
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Idle check interval" }))
      .toHaveValue("45");
    await expect.element(page.getByText("Changed elsewhere")).toBeInTheDocument();
    await expect
      .element(page.getByText("Changed by someone else while you were editing. It was 30 seconds."))
      .toBeInTheDocument();
    expect(server.values.enrichment_concurrency).toBe(6);
    expect(server.values.enrichment_poll_idle_secs).toBe(45);
  });

  test("the changed-elsewhere note goes away on the next edit", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_poll_idle_secs = 45;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await expect.element(page.getByText("Changed elsewhere")).toBeInTheDocument();
    await type(user, "Concurrent lookups", "7");
    await expect.element(page.getByText("Changed elsewhere")).not.toBeInTheDocument();
  });

  test("the reapplying notice shows while the reload is in flight", async () => {
    server.stale.set(1, () => {
      server.revision += 1;
    });
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const user = userEvent;
    await loaded();
    const realFetch = vi.mocked(globalThis.fetch).getMockImplementation();
    vi.mocked(globalThis.fetch).mockImplementation(async (input, init) => {
      if (
        (init?.method ?? "GET") === "GET" &&
        server.puts.length > 0 &&
        urlOf(input).pathname === "/api/v1/settings"
      )
        await gate;
      return realFetch === undefined ? new Response() : realFetch(input, init);
    });
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were saving"))
      .toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveAttribute("readonly");
    release();
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
  });

  test("a 428 is reconciled the same way as a 412", async () => {
    server.putMode = "428-first";
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
  });

  test("a server value equal to the user's own value is not a conflict and is not sent again", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 6;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .not.toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    await expect.element(page.getByText(/^Saved at .*\d.*\.$/)).toBeInTheDocument();
    await expect.element(save()).toBeDisabled();
  });

  test("agreement on one field still reapplies the others", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 6;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts[1]?.patch).toEqual({ enrichment_fetch_budget_secs: 30 });
  });

  test("an overlapping field opens the conflict panel and sends nothing more", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.values.enrichment_poll_idle_secs = 45;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    const alert = page.getByText("Settings changed while you were editing");
    await expect.element(alert).toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    await expect
      .element(
        page.getByText("Someone else saved 8 lookups while you were editing. You set 6 lookups."),
      )
      .toBeInTheDocument();
    await expect.element(page.getByRole("radio", { name: "Keep mine (6 lookups)" })).toBeChecked();
    await expect
      .element(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))
      .not.toBeChecked();
    await expect.element(page.getByText("Conflict")).toBeInTheDocument();
    await expect.element(page.getByText("Changed elsewhere")).toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Idle check interval" }))
      .toHaveValue("45");
    await expect.element(page.getByText("Not saved.")).toBeInTheDocument();
    await expect
      .element(page.getByText(/1 setting was changed by someone else\./))
      .toBeInTheDocument();
  });

  test("Save my choices with Keep mine sends the user's value against the new tag", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Save my choices" }));
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts[1]).toEqual({
      patch: { enrichment_concurrency: 6, enrichment_fetch_budget_secs: 30 },
      ifMatch: '"rev-2"',
    });
    expect(server.values.enrichment_concurrency).toBe(6);
  });

  test("Use theirs drops that field from the patch and shows their value", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    (await rendered(page.getByRole("radio", { name: "Keep mine (6 lookups)" }))).focus();
    await user.keyboard("{ArrowRight}");
    await expect.element(page.getByRole("radio", { name: "Use theirs (8 lookups)" })).toBeChecked();
    await expect.element(page.getByRole("radio", { name: "Use theirs (8 lookups)" })).toHaveFocus();
    await user.click(page.getByRole("button", { name: "Save my choices" }));
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts[1]?.patch).toEqual({ enrichment_fetch_budget_secs: 30 });
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("8");
  });

  test("editing a field after choosing theirs switches it back to mine and saves the new value", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    await user.click(
      page.elementLocator(
        (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
          "label",
        ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
      ),
    );
    await type(user, "Concurrent lookups", "9");
    await expect.element(page.getByRole("radio", { name: "Keep mine (9 lookups)" })).toBeChecked();
    await expect
      .element(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))
      .not.toBeChecked();
    await expect
      .element(
        page.getByText("Someone else saved 8 lookups while you were editing. You set 9 lookups."),
      )
      .toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Save my choices" }));
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts[1]?.patch).toEqual({
      enrichment_concurrency: 9,
      enrichment_fetch_budget_secs: 30,
    });
    expect(server.values.enrichment_concurrency).toBe(9);
  });

  describe("with invalid local text in a conflicted field", () => {
    async function conflictWithInvalidText(
      user: typeof userEvent,
      editOthers: boolean,
    ): Promise<Locator> {
      server.stale.set(1, () => {
        server.values.enrichment_concurrency = 8;
        server.revision += 1;
      });
      await loaded();
      await type(user, "Concurrent lookups", "6");
      if (editOthers) await type(user, "Time allowed per source", "30");
      await user.click(save());
      await expect
        .element(page.getByText("Settings changed while you were editing"))
        .toBeInTheDocument();
      await type(user, "Concurrent lookups", "abc");
      await user.tab();
      const field = page.getByRole("textbox", { name: "Concurrent lookups" });
      await expect.element(field).toHaveAttribute("aria-invalid", "true");
      await expect.element(field).toHaveAccessibleDescription(/Enter a number\./);
      return field;
    }

    test("Use theirs lets Save my choices through, clears the discarded error and keeps other edits", async () => {
      const user = userEvent;
      const field = await conflictWithInvalidText(user, true);
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
            "label",
          ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
        ),
      );
      await user.click(page.getByRole("button", { name: "Save my choices" }));
      await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
      expect(server.puts).toHaveLength(2);
      expect(server.puts[1]).toEqual({
        patch: { enrichment_fetch_budget_secs: 30 },
        ifMatch: '"rev-2"',
      });
      await expect.element(field).toHaveValue("8");
      await expect.element(field).not.toHaveAttribute("aria-invalid");
      await expect.element(field).not.toHaveAccessibleDescription(/Enter a number/);
      await expect
        .element(page.getByRole("textbox", { name: "Time allowed per source" }))
        .toHaveValue("30");
    });

    test("Use theirs with nothing else edited clears the error and the marker without a request", async () => {
      const user = userEvent;
      const field = await conflictWithInvalidText(user, false);
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
            "label",
          ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
        ),
      );
      await user.click(page.getByRole("button", { name: "Save my choices" }));
      await vi.waitFor(async () => {
        await expect
          .element(page.getByText("Settings changed while you were editing"))
          .not.toBeInTheDocument();
      });
      expect(server.puts).toHaveLength(1);
      await expect.element(field).toHaveValue("8");
      await expect.element(field).not.toHaveAttribute("aria-invalid");
      await expect.element(field).not.toHaveAccessibleDescription(/Enter a number/);
      await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
    });

    test("a retained invalid field still blocks Save my choices", async () => {
      const user = userEvent;
      await conflictWithInvalidText(user, true);
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
            "label",
          ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
        ),
      );
      await type(user, "Time allowed per source", "0");
      await user.click(page.getByRole("button", { name: "Save my choices" }));
      const other = page.getByRole("textbox", { name: "Time allowed per source" });
      await vi.waitFor(async () => {
        await expect.element(other).toHaveFocus();
      });
      await expect.element(other).toHaveAttribute("aria-invalid", "true");
      expect(server.puts).toHaveLength(1);
      await expect
        .element(page.getByText("Settings changed while you were editing"))
        .toBeInTheDocument();
      await type(user, "Time allowed per source", "31");
      await user.click(page.getByRole("button", { name: "Save my choices" }));
      await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
      expect(server.puts[1]?.patch).toEqual({ enrichment_fetch_budget_secs: 31 });
      await expect
        .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
        .toHaveValue("8");
      expect(invalidControls()).toHaveLength(0);
    });

    test("a failed Save my choices keeps the other edit, clears the discarded error and still blocks invalid retained text", async () => {
      const user = userEvent;
      const field = await conflictWithInvalidText(user, true);
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
            "label",
          ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
        ),
      );
      server.putMode = "500";
      await user.click(page.getByRole("button", { name: "Save my choices" }));
      await expect
        .element(page.getByText("The server returned an error. Your edits are still here."))
        .toBeInTheDocument();
      expect(server.puts).toHaveLength(2);
      await expect.element(field).toHaveValue("8");
      await expect.element(field).not.toHaveAttribute("aria-invalid");
      await expect.element(field).not.toHaveAccessibleDescription(/Enter a number/);
      const other = page.getByRole("textbox", { name: "Time allowed per source" });
      await expect.element(other).toHaveValue("30");
      await type(user, "Time allowed per source", "0");
      await user.click(save());
      await vi.waitFor(async () => {
        await expect.element(other).toHaveFocus();
      });
      await expect.element(other).toHaveAttribute("aria-invalid", "true");
      expect(server.puts).toHaveLength(2);
    });

    test("choosing Keep mine again restores the field's validation", async () => {
      const user = userEvent;
      const field = await conflictWithInvalidText(user, true);
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
            "label",
          ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
        ),
      );
      await user.click(
        page.elementLocator(
          (await rendered(page.getByRole("radio", { name: "Keep mine (abc)" }))).closest("label") ??
            (await rendered(page.getByRole("radio", { name: "Keep mine (abc)" }))),
        ),
      );
      await user.click(page.getByRole("button", { name: "Save my choices" }));
      await vi.waitFor(async () => {
        await expect.element(field).toHaveFocus();
      });
      await expect.element(field).toHaveAttribute("aria-invalid", "true");
      expect(server.puts).toHaveLength(1);
    });
  });

  test.each(["invalid JSON", "invalid schema"])(
    "a write response with %s leaves the held values and tag together",
    async (failure) => {
      const user = userEvent;
      await loaded();
      await type(user, "Concurrent lookups", "6");
      const realFetch = vi.mocked(globalThis.fetch).getMockImplementation();
      vi.mocked(globalThis.fetch).mockImplementation(async (input, init) => {
        const response = realFetch === undefined ? new Response() : await realFetch(input, init);
        if (urlOf(input).pathname === "/api/v1/settings" && init?.method === "PUT") {
          return new Response(failure === "invalid JSON" ? "{" : "{}", {
            headers: { "Content-Type": "application/json", ETag: etag() },
          });
        }
        return response;
      });
      await user.click(save());
      await expect
        .element(page.getByText("The server returned an error. Your edits are still here."))
        .toBeInTheDocument();
      await expect
        .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
        .toHaveValue("6");
      installFetch();
      await user.click(save());
      await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
      expect(server.puts).toHaveLength(2);
      expect(server.puts[1]?.ifMatch).toBe('"rev-1"');
      expect(server.values.enrichment_concurrency).toBe(6);
      expect(server.revision).toBe(2);
    },
  );

  test("a failed reload after a stale answer cannot carry the newer tag over the old values", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
      server.getStatus = 503;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("The server returned an error. Your edits are still here."))
      .toBeInTheDocument();
    server.getStatus = 200;
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
    expect(server.puts[1]?.ifMatch).toBe('"rev-1"');
    expect(server.values.enrichment_concurrency).toBe(8);
  });

  test("a failed reload after the reapply is stale again cannot carry the newer tag either", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_poll_idle_secs = 45;
      server.revision += 1;
    });
    server.stale.set(2, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
      server.getStatus = 503;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("The server returned an error. Your edits are still here."))
      .toBeInTheDocument();
    server.getStatus = 200;
    server.stale.clear();
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    expect(server.puts).toHaveLength(3);
    expect(server.puts[2]?.ifMatch).toBe('"rev-2"');
    expect(server.values.enrichment_concurrency).toBe(8);
  });

  test("choosing theirs for every conflict sends nothing and clears the panel", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    await user.click(
      page.elementLocator(
        (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))).closest(
          "label",
        ) ?? (await rendered(page.getByRole("radio", { name: "Use theirs (8 lookups)" }))),
      ),
    );
    await user.click(page.getByRole("button", { name: "Save my choices" }));
    await vi.waitFor(async () => {
      await expect
        .element(page.getByText("Settings changed while you were editing"))
        .not.toBeInTheDocument();
    });
    expect(server.puts).toHaveLength(1);
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("8");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("Discard mine and reload drops the draft and shows the server's values", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Discard mine and reload" }));
    await vi.waitFor(async () => {
      await expect
        .element(page.getByText("Settings changed while you were editing"))
        .not.toBeInTheDocument();
    });
    await expect
      .element(page.getByRole("textbox", { name: "Concurrent lookups" }))
      .toHaveValue("8");
    await expect.element(page.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("Discard mine and reload becomes natively disabled after discarding", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    {
      (await rendered(page.getByRole("button", { name: "Discard mine and reload" }))).focus();
    }
    await user.keyboard("{Enter}");
    await vi.waitFor(async () => {
      await expect
        .element(page.getByText("Settings changed while you were editing"))
        .not.toBeInTheDocument();
    });
    const discard = page.getByRole("button", { name: "Discard changes" });
    await expect.element(discard).toBeDisabled();
    await user.tab();
    await expect.element(discard).not.toHaveFocus();
  });

  test("the retry is bounded: a second stale answer opens the conflict panel after exactly two writes", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_poll_idle_secs = 45;
      server.revision += 1;
    });
    server.stale.set(2, () => {
      server.values.enrichment_poll_idle_secs = 50;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
    await expect
      .element(page.getByRole("textbox", { name: "Idle check interval" }))
      .toHaveValue("50");
    await expect.element(page.getByRole("radio")).not.toBeInTheDocument();
    await expect
      .element(page.getByText(/Settings were changed by someone else\./))
      .toBeInTheDocument();
    server.stale.clear();
    await user.click(page.getByRole("button", { name: "Save my choices" }));
    await expect.element(page.getByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(3);
    expect(server.values.enrichment_concurrency).toBe(6);
  });

  test("the conflict panel keeps the draft when leaving is attempted", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent;
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await expect
      .element(page.getByText("Settings changed while you were editing"))
      .toBeInTheDocument();
    await user.click(page.getByRole("link", { name: /Covers/ }));
    await expect.element(page.getByRole("alertdialog")).toBeInTheDocument();
  });
});

async function rendered(locator: Locator): Promise<HTMLElement | SVGElement> {
  await expect.element(locator).toBeInTheDocument();
  return locator.element();
}
