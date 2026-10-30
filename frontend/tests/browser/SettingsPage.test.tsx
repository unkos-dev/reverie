import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import type { ReactElement } from "react";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { z } from "zod";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __resetEtagCacheForTesting } from "@/api/etags";
import { queryClient as appQueryClient } from "@/lib/query/client";

import { adminSettingsRoute } from "@/routes/production";

import { SettingsPage } from "@/pages/admin/settings/SettingsPage";

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

type Server = {
  values: Values;
  revision: number;
  getStatus: number;
  /** Revisions at which a PUT is answered 412 (the PUT count, 1-based), with a hook to move the data first. */
  stale: Map<number, () => void>;
  puts: PutRecord[];
};

let server: Server;

function newServer(): Server {
  return { values: { ...BASE_VALUES }, revision: 1, getStatus: 200, stale: new Map(), puts: [] };
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
  const hook = server.stale.get(server.puts.length);
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
          role: "admin",
          is_child: false,
          has_local_password: true,
          theme_preference: "system",
          csrf_token: null,
        }),
      );
    }
    if (url.pathname === "/api/v1/settings/reload-status") {
      return Promise.resolve(reply({ last_successful_reload_at: "2026-10-09T10:42:00Z" }));
    }
    if (url.pathname === "/api/v1/settings" && method === "GET") {
      if (server.getStatus !== 200) {
        return Promise.resolve(
          reply({ title: "Error", status: server.getStatus }, server.getStatus),
        );
      }
      return Promise.resolve(
        reply({ ...row(), restart_required_fields: [] }, 200, { ETag: etag() }),
      );
    }
    if (url.pathname === "/api/v1/settings" && method === "PUT") {
      return Promise.resolve(handlePut(init));
    }
    return Promise.resolve(reply({ title: "Not Found", status: 404 }, 404));
  });
}

async function renderSettings(
  area = "enrichment",
  client: QueryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  route: RouteObject = { path: "/admin/settings/:area", element: <SettingsPage /> },
): Promise<{ router: ReturnType<typeof createMemoryRouter> }> {
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
  return { router };
}

async function loaded(area = "enrichment"): Promise<void> {
  await renderSettings(area);
  await expect.element(page.getByRole("form", { name: /settings$/ })).toBeInTheDocument();
}

async function type(user: typeof userEvent, label: string, text: string): Promise<void> {
  const input = page.getByRole("textbox", { name: label });
  await input.fill("");
  await user.type(input, text);
}

const save = (): Locator => page.getByRole("button", { name: /^Save/ });

async function rendered(locator: Locator): Promise<HTMLElement | SVGElement> {
  await expect.element(locator).toBeInTheDocument();
  return locator.element();
}

beforeEach(() => {
  server = newServer();
  __resetEtagCacheForTesting();
  installFetch();
});

afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
});

describe("production settings navigation", () => {
  beforeEach(() => {
    appQueryClient.clear();
  });

  afterEach(() => {
    appQueryClient.clear();
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
});
