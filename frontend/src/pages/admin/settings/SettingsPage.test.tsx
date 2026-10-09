import { QueryCache, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { RouterProvider, createMemoryRouter } from "react-router";
import { z } from "zod";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError } from "@/api/errors";
import { __resetEtagCacheForTesting } from "@/api/etags";
import {
  invokeUnauthenticatedHandler,
  queryClient as appQueryClient,
  setUnauthenticatedHandler,
} from "@/lib/query/client";

import { SettingsPage } from "./SettingsPage";

const BodySchema = z.record(z.string(), z.unknown());

type Values = Record<string, unknown>;

function urlOf(input: RequestInfo | URL): URL {
  const raw = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
  return new URL(raw, "http://localhost");
}

const BASE_VALUES: Values = {
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

type PutMode = "ok" | "403" | "422" | "422-unmapped" | "network" | "500" | "401" | "428-first";

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

function renderSettings(
  area = "enrichment",
  client: QueryClient = newClient(),
): {
  router: ReturnType<typeof createMemoryRouter>;
  client: QueryClient;
} {
  const router = createMemoryRouter(
    [
      { path: "/admin/settings/:area", element: <SettingsPage /> },
      { path: "/library", element: <div>Library page</div> },
    ],
    { initialEntries: [`/admin/settings/${area}`] },
  );
  function Harness(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Harness />);
  return { router, client };
}

async function loaded(area = "enrichment"): Promise<ReturnType<typeof renderSettings>> {
  const rendered = renderSettings(area);
  await screen.findByRole("form", { name: /settings$/ });
  return rendered;
}

async function type(
  user: ReturnType<typeof userEvent.setup>,
  label: string,
  text: string,
): Promise<void> {
  const input = screen.getByRole("textbox", { name: label });
  await user.clear(input);
  await user.type(input, text);
}

const save = (): HTMLElement => screen.getByRole("button", { name: /^Save/ });

beforeEach(() => {
  server = newServer();
  __resetEtagCacheForTesting();
  installFetch();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  setUnauthenticatedHandler(() => {});
});

describe("access", () => {
  test("a non-admin sees the administrators notice and no settings request is made", async () => {
    server = newServer({ role: "adult" });
    renderSettings();
    expect(await screen.findByText("Settings are for administrators")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to the library" })).toHaveAttribute(
      "href",
      "/library",
    );
    expect(screen.queryByRole("form")).not.toBeInTheDocument();
    expect(server.gets).toBe(0);
  });

  test("a 403 on the settings read shows the same notice for an admin whose role lapsed", async () => {
    server = newServer({ getStatus: 403 });
    renderSettings();
    expect(await screen.findByText("Settings are for administrators")).toBeInTheDocument();
  });

  test("an unknown or unshipped area redirects to the first area", async () => {
    const { router } = renderSettings("providers");
    await screen.findByRole("form", { name: "Acquisition settings" });
    expect(router.state.location.pathname).toBe("/admin/settings/acquisition");
  });

  test("a 401 on load reaches the sign-in handler through the application's query client", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);
    server = newServer({ getStatus: 401 });
    appQueryClient.clear();
    renderSettings("enrichment", appQueryClient);
    await waitFor(() => {
      expect(handler).toHaveBeenCalledTimes(1);
    });
    appQueryClient.clear();
  });

  test("a 401 on save goes through the global sign-in handler and keeps the edits", async () => {
    const handler = vi.fn();
    setUnauthenticatedHandler(handler);
    server.putMode = "401";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await waitFor(() => {
      expect(handler).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("6");
  });
});

describe("loading", () => {
  test("shows the page head and a busy loading region before the data arrives", async () => {
    renderSettings();
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(await screen.findByText("Loading settings")).toBeInTheDocument();
    expect(screen.getByRole("status", { busy: true })).toBeInTheDocument();
    await screen.findByRole("form", { name: "Enrichment settings" });
  });

  test("a failed load shows a recoverable error and Try again fetches again", async () => {
    server = newServer({ getStatus: 503 });
    const user = userEvent.setup();
    renderSettings();
    expect(await screen.findByText("Settings could not be loaded")).toBeInTheDocument();
    expect(screen.getByText(/Nothing has been changed/)).toBeInTheDocument();
    server.getStatus = 200;
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("form", { name: "Enrichment settings" })).toBeInTheDocument();
  });

  test("renders the area's fields, the live sub-navigation summaries and the status line", async () => {
    await loaded();
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("4");
    expect(
      screen.getByRole("switch", { name: "Fetch metadata from external sources" }),
    ).toBeChecked();
    const nav = screen.getByRole("navigation", { name: "Settings areas" });
    expect(within(nav).getByRole("link", { name: /Enrichment/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(within(nav).getByText("On. 4 lookups at a time")).toBeInTheDocument();
    expect(within(nav).getByText("Up to 10 MiB")).toBeInTheDocument();
    expect(within(nav).queryByText(/Provider display/)).not.toBeInTheDocument();
    expect(screen.getByText("Last changed")).toBeInTheDocument();
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Discard changes" })).toBeDisabled();
    expect(save()).toBeDisabled();
  });

  test("a null reload time reads as the normal never-refreshed state", async () => {
    server = newServer({ reload: null });
    await loaded();
    expect(await screen.findByText(/Not refreshed since the server started/)).toBeInTheDocument();
  });

  test("a failing reload-status read leaves the settings usable and omits that line", async () => {
    server = newServer({ reloadStatus: 500 });
    await loaded();
    expect(screen.getByText("Last changed")).toBeInTheDocument();
    expect(screen.queryByText("Server cache refreshed")).not.toBeInTheDocument();
  });

  test("the MiB field shows the exact byte count beneath it", async () => {
    await loaded("covers");
    expect(screen.getByText("10,485,760 bytes")).toBeInTheDocument();
    const user = userEvent.setup();
    await type(user, "Largest cover file", "0.5");
    expect(screen.getByText("524,288 bytes")).toBeInTheDocument();
  });
});

describe("editing", () => {
  test("an edit marks the row, the sub-navigation item and the bar; Discard reverts it", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    expect(screen.getByText("Saved value: 4 lookups")).toBeInTheDocument();
    expect(screen.getByText("1 unsaved change")).toBeInTheDocument();
    const nav = screen.getByRole("navigation", { name: "Settings areas" });
    expect(within(nav).getByText("Edited")).toBeInTheDocument();
    await type(user, "Time allowed per source", "30");
    expect(screen.getByText("2 unsaved changes")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("4");
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
    expect(screen.queryByText("Edited")).not.toBeInTheDocument();
  });

  test("editing back to the saved value clears the dirty state", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await type(user, "Concurrent lookups", "4");
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
    expect(save()).toBeDisabled();
  });

  test("an area's edits never appear in another area's bar", async () => {
    const user = userEvent.setup();
    const { router } = await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    await user.click(await screen.findByRole("button", { name: "Leave and discard changes" }));
    await screen.findByRole("form", { name: "Covers settings" });
    expect(router.state.location.pathname).toBe("/admin/settings/covers");
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("an empty accepted-formats list warns that importing will stop and still saves", async () => {
    const user = userEvent.setup();
    await loaded("acquisition");
    await user.click(screen.getByRole("checkbox", { name: "EPUB" }));
    expect(screen.getByText("Importing will stop")).toBeInTheDocument();
    await user.click(save());
    await screen.findByText("Settings saved");
    expect(server.puts[0]?.patch).toEqual({ accepted_formats: [] });
  });
});

describe("saving", () => {
  test("sends only the dirty fields with the tag the read returned, then reports the save", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await type(user, "Time allowed per source", "30");
    await user.click(save());
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(
      screen.getByText("Changes are in effect now. No restart is needed."),
    ).toBeInTheDocument();
    expect(server.puts).toEqual([
      {
        patch: { enrichment_concurrency: 6, enrichment_fetch_budget_secs: 30 },
        ifMatch: '"rev-1"',
      },
    ]);
    expect(screen.getByText(/^Saved at \d\d:\d\d\.$/)).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("6");
    const nav = screen.getByRole("navigation", { name: "Settings areas" });
    expect(within(nav).getByText("On. 6 lookups at a time")).toBeInTheDocument();
    expect(screen.queryByText("Edited")).not.toBeInTheDocument();
  });

  test("a second save carries the tag the first save returned", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings saved");
    await type(user, "Concurrent lookups", "7");
    await user.click(save());
    await waitFor(() => {
      expect(server.puts).toHaveLength(2);
    });
    expect(server.puts[1]?.ifMatch).toBe('"rev-2"');
  });

  test("a read slow enough to land after another admin's save never rebases an active draft", async () => {
    const user = userEvent.setup();
    await loaded();
    let release = (): void => {};
    server.getGate = new Promise<void>((resolve) => {
      release = resolve;
    });
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings saved");
    await type(user, "Concurrent lookups", "7");
    server.values.enrichment_concurrency = 8;
    server.revision += 1;
    server.getGate = null;
    release();
    await new Promise((resolve) => setTimeout(resolve, 50));
    await user.click(save());
    expect(await screen.findByText("Settings changed while you were editing")).toBeInTheDocument();
    expect(server.puts[1]?.ifMatch).toBe('"rev-2"');
    expect(server.values.enrichment_concurrency).toBe(8);
  });

  test("an edit after a save clears the saved notice", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings saved");
    await type(user, "Concurrent lookups", "7");
    expect(screen.queryByText("Settings saved")).not.toBeInTheDocument();
  });

  test("never sends an empty patch", async () => {
    const user = userEvent.setup();
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
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    const busy = await screen.findByRole("button", { name: "Saving" });
    expect(busy).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toBeDisabled();
    expect(screen.getByText("Saving 1 change")).toBeInTheDocument();
    await user.click(busy);
    release();
    await screen.findByText("Settings saved");
    expect(server.puts).toHaveLength(1);
  });
});

describe("validation", () => {
  test("client bounds block the save, mark each field and link to it", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await type(user, "Idle check interval", "0");
    await user.click(save());
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Settings were not saved")).toBeInTheDocument();
    expect(
      within(alert).getByText(
        /2 fields need attention\. Fix them and save again\. Nothing you typed has been lost\./,
      ),
    ).toBeInTheDocument();
    expect(alert).toHaveFocus();
    expect(server.puts).toHaveLength(0);
    const concurrency = screen.getByRole("textbox", { name: "Concurrent lookups" });
    expect(concurrency).toHaveAttribute("aria-invalid", "true");
    expect(concurrency).toHaveAccessibleDescription(/Enter a whole number from 1 to 10\./);
    expect(
      screen.getByRole("textbox", { name: "Idle check interval" }),
    ).toHaveAccessibleDescription(/Enter a whole number of 1 or more\./);
    expect(screen.getByText("Not saved.").parentElement).toHaveTextContent(
      "Not saved. 2 fields need attention.",
    );
    expect(save()).toBeEnabled();
    await user.click(within(alert).getByRole("button", { name: "Go to Idle check interval" }));
    expect(screen.getByRole("textbox", { name: "Idle check interval" })).toHaveFocus();
  });

  test("fixing a field clears its error and the last fix clears the notice", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "12");
    await user.click(save());
    await screen.findByRole("alert");
    await type(user, "Concurrent lookups", "8");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).not.toHaveAttribute(
      "aria-invalid",
    );
  });

  test("non-numeric text reports Enter a number", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "many");
    await user.click(save());
    expect(await screen.findByText("Enter a number.")).toBeInTheDocument();
  });

  test("a server 422 that names a field lands on that field as written", async () => {
    server.putMode = "422";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    const field = screen.getByRole("textbox", { name: "Concurrent lookups" });
    await waitFor(() => {
      expect(field).toHaveAttribute("aria-invalid", "true");
    });
    expect(field).toHaveAccessibleDescription(/enrichment_concurrency must be between 1 and 10/);
    expect(field).toHaveValue("6");
  });

  test("a server 422 that names no field shows the unattributed message", async () => {
    server.putMode = "422-unmapped";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(
      await screen.findByText(
        "The server did not accept this change: request body is not acceptable.",
      ),
    ).toBeInTheDocument();
  });
});

describe("save failures", () => {
  test("a 403 keeps the edits, says nothing was saved and leaves without the discard prompt", async () => {
    server.putMode = "403";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(await screen.findByText("Nothing was saved")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("6");
    await user.click(screen.getByRole("link", { name: "Leave settings" }));
    expect(await screen.findByText("Library page")).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("a network failure keeps the edits and says the server could not be reached", async () => {
    server.putMode = "network";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(
      await screen.findByText("The server could not be reached. Your edits are still here."),
    ).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("6");
    expect(save()).toBeEnabled();
  });

  test("a server error keeps the edits and can be retried", async () => {
    server.putMode = "500";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(
      await screen.findByText("The server returned an error. Your edits are still here."),
    ).toBeInTheDocument();
    server.putMode = "ok";
    await user.click(save());
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
  });
});

describe("leaving with unsaved edits", () => {
  test("Stay keeps the page and the draft; focus lands on Stay first", async () => {
    const user = userEvent.setup();
    const { router } = await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("Leave without saving?")).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        /You have 1 unsaved change in Enrichment\. It will be lost if you leave\./,
      ),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Stay on this page" })).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: "Stay on this page" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/admin/settings/enrichment");
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("6");
  });

  test("Escape means Stay", async () => {
    const user = userEvent.setup();
    const { router } = await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    await screen.findByRole("alertdialog");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });
    expect(router.state.location.pathname).toBe("/admin/settings/enrichment");
  });

  test("Leave discards the draft; coming back shows the saved value", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await type(user, "Time allowed per source", "30");
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    expect(
      await screen.findByText(
        /You have 2 unsaved changes in Enrichment\. They will be lost if you leave\./,
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Leave and discard changes" }));
    await screen.findByRole("form", { name: "Covers settings" });
    await user.click(screen.getByRole("link", { name: /Enrichment/ }));
    await screen.findByRole("form", { name: "Enrichment settings" });
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("4");
  });

  test("a pristine page navigates away with no prompt", async () => {
    const user = userEvent.setup();
    await loaded();
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    await screen.findByRole("form", { name: "Covers settings" });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("after a successful save the page is clean and navigation is not blocked", async () => {
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings saved");
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    await screen.findByRole("form", { name: "Covers settings" });
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  test("registers a beforeunload warning only while the page has unsaved edits", async () => {
    const add = vi.spyOn(window, "addEventListener");
    const user = userEvent.setup();
    await loaded();
    expect(add.mock.calls.filter(([type]) => type === "beforeunload")).toHaveLength(0);
    await type(user, "Concurrent lookups", "6");
    expect(add.mock.calls.filter(([type]) => type === "beforeunload")).toHaveLength(1);
    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    await user.click(screen.getByRole("button", { name: "Discard changes" }));
    const after = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(after);
    expect(after.defaultPrevented).toBe(false);
  });
});

describe("source-file deletion switches", () => {
  test("turning one on asks first; Keep leaves it off and returns focus to the switch", async () => {
    const user = userEvent.setup();
    await loaded("acquisition");
    const toggle = screen.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(toggle);
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("Remove source files for duplicates?")).toBeInTheDocument();
    expect(
      within(dialog).getByText(/This cannot be undone\. The setting takes effect when you save\./),
    ).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Keep source files" })).toHaveFocus();
    expect(toggle).not.toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Keep source files" }));
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });
    expect(toggle).not.toBeChecked();
    expect(toggle).toHaveFocus();
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("Escape behaves as Keep", async () => {
    const user = userEvent.setup();
    await loaded("acquisition");
    const toggle = screen.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(toggle);
    await screen.findByRole("alertdialog");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });
    expect(toggle).not.toBeChecked();
  });

  test("confirming turns it on, marks the row edited and states the consequence; nothing is saved yet", async () => {
    const user = userEvent.setup();
    await loaded("acquisition");
    const toggle = screen.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(toggle);
    await user.click(await screen.findByRole("button", { name: "Turn on removal" }));
    await waitFor(() => {
      expect(toggle).toBeChecked();
    });
    expect(
      screen.getByText(
        /When you save, Reverie starts deleting the original file from the ingestion folder when the library already holds the same book\. This cannot be undone\./,
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("1 unsaved change")).toBeInTheDocument();
    expect(server.puts).toHaveLength(0);
    await user.click(save());
    await screen.findByText("Settings saved");
    expect(server.puts[0]?.patch).toEqual({ cleanup_duplicates: true });
  });

  test("the import switch has its own wording", async () => {
    server = newServer({ values: { ...BASE_VALUES, cleanup_imported: false } });
    const user = userEvent.setup();
    await loaded("acquisition");
    await user.click(
      screen.getByRole("switch", { name: "Remove the source file after a successful import" }),
    );
    expect(await screen.findByText("Remove source files after import?")).toBeInTheDocument();
  });

  test("turning one off needs no confirmation", async () => {
    const user = userEvent.setup();
    await loaded("acquisition");
    await user.click(
      screen.getByRole("switch", { name: "Remove the source file after a successful import" }),
    );
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByText("1 unsaved change")).toBeInTheDocument();
  });

  test("turning one on and back off before saving leaves nothing dirty", async () => {
    const user = userEvent.setup();
    await loaded("acquisition");
    const toggle = screen.getByRole("switch", {
      name: "Remove the source file when the book is a duplicate",
    });
    await user.click(toggle);
    await user.click(await screen.findByRole("button", { name: "Turn on removal" }));
    await waitFor(() => {
      expect(toggle).toBeChecked();
    });
    await user.click(toggle);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("a switch that deletes nothing never asks", async () => {
    const user = userEvent.setup();
    server = newServer({ values: { ...BASE_VALUES, enrichment_enabled: false } });
    await loaded();
    await user.click(screen.getByRole("switch", { name: "Fetch metadata from external sources" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: "Fetch metadata from external sources" }),
    ).toBeChecked();
  });
});

describe("a stale write", () => {
  async function editTwoFields(user: ReturnType<typeof userEvent.setup>): Promise<void> {
    await type(user, "Concurrent lookups", "6");
    await type(user, "Time allowed per source", "30");
  }

  test("with no overlapping field it reloads, reapplies once, and says what the other admin changed", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_poll_idle_secs = 45;
      server.values.writeback_max_attempts = 3;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
    expect(server.puts[0]?.ifMatch).toBe('"rev-1"');
    expect(server.puts[1]).toEqual({
      patch: { enrichment_concurrency: 6, enrichment_fetch_budget_secs: 30 },
      ifMatch: '"rev-2"',
    });
    expect(screen.getByText(/None of your changes overlapped with theirs/)).toBeInTheDocument();
    expect(
      screen.getByText(
        "Also changed by someone else: Writeback, Attempts per file, now 3 attempts.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Idle check interval" })).toHaveValue("45");
    expect(screen.getByText("Changed elsewhere")).toBeInTheDocument();
    expect(
      screen.getByText("Changed by someone else while you were editing. It was 30 seconds."),
    ).toBeInTheDocument();
    expect(server.values.enrichment_concurrency).toBe(6);
    expect(server.values.enrichment_poll_idle_secs).toBe(45);
  });

  test("the changed-elsewhere note goes away on the next edit", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_poll_idle_secs = 45;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await screen.findByText("Changed elsewhere");
    await type(user, "Concurrent lookups", "7");
    expect(screen.queryByText("Changed elsewhere")).not.toBeInTheDocument();
  });

  test("the reapplying notice shows while the reload is in flight", async () => {
    server.stale.set(1, () => {
      server.revision += 1;
    });
    let release: () => void = () => undefined;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const user = userEvent.setup();
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
    expect(await screen.findByText("Settings changed while you were saving")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toBeDisabled();
    release();
    await screen.findByText("Settings saved");
  });

  test("a 428 is reconciled the same way as a 412", async () => {
    server.putMode = "428-first";
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
  });

  test("a server value equal to the user's own value is not a conflict and is not sent again", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 6;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
    expect(screen.queryByText("Settings changed while you were editing")).not.toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    expect(screen.getByText(/^Saved at \d\d:\d\d\.$/)).toBeInTheDocument();
    expect(save()).toBeDisabled();
  });

  test("agreement on one field still reapplies the others", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 6;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await screen.findByText("Settings saved");
    expect(server.puts[1]?.patch).toEqual({ enrichment_fetch_budget_secs: 30 });
  });

  test("an overlapping field opens the conflict panel and sends nothing more", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.values.enrichment_poll_idle_secs = 45;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    const alert = await screen.findByText("Settings changed while you were editing");
    expect(alert).toBeInTheDocument();
    expect(server.puts).toHaveLength(1);
    expect(
      screen.getByText("Someone else saved 8 lookups while you were editing. You set 6 lookups."),
    ).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Keep mine (6 lookups)" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Use theirs (8 lookups)" })).not.toBeChecked();
    expect(screen.getByText("Conflict")).toBeInTheDocument();
    expect(screen.getByText("Changed elsewhere")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Idle check interval" })).toHaveValue("45");
    expect(screen.getByText("Not saved.")).toBeInTheDocument();
    expect(screen.getByText(/1 setting was changed by someone else\./)).toBeInTheDocument();
  });

  test("Save my choices with Keep mine sends the user's value against the new tag", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await screen.findByText("Settings changed while you were editing");
    await user.click(screen.getByRole("button", { name: "Save my choices" }));
    expect(await screen.findByText("Settings saved")).toBeInTheDocument();
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
    const user = userEvent.setup();
    await loaded();
    await editTwoFields(user);
    await user.click(save());
    await screen.findByText("Settings changed while you were editing");
    await user.click(screen.getByRole("radio", { name: "Use theirs (8 lookups)" }));
    await user.click(screen.getByRole("button", { name: "Save my choices" }));
    await screen.findByText("Settings saved");
    expect(server.puts[1]?.patch).toEqual({ enrichment_fetch_budget_secs: 30 });
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("8");
  });

  test("choosing theirs for every conflict sends nothing and clears the panel", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings changed while you were editing");
    await user.click(screen.getByRole("radio", { name: "Use theirs (8 lookups)" }));
    await user.click(screen.getByRole("button", { name: "Save my choices" }));
    await waitFor(() => {
      expect(screen.queryByText("Settings changed while you were editing")).not.toBeInTheDocument();
    });
    expect(server.puts).toHaveLength(1);
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("8");
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
  });

  test("Discard mine and reload drops the draft and shows the server's values", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings changed while you were editing");
    await user.click(screen.getByRole("button", { name: "Discard mine and reload" }));
    await waitFor(() => {
      expect(screen.queryByText("Settings changed while you were editing")).not.toBeInTheDocument();
    });
    expect(screen.getByRole("textbox", { name: "Concurrent lookups" })).toHaveValue("8");
    expect(screen.getByText("No unsaved changes.")).toBeInTheDocument();
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
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    expect(await screen.findByText("Settings changed while you were editing")).toBeInTheDocument();
    expect(server.puts).toHaveLength(2);
    expect(screen.getByRole("textbox", { name: "Idle check interval" })).toHaveValue("50");
    expect(screen.queryByRole("radio")).not.toBeInTheDocument();
    expect(screen.getByText(/Settings were changed by someone else\./)).toBeInTheDocument();
    server.stale.clear();
    await user.click(screen.getByRole("button", { name: "Save my choices" }));
    await screen.findByText("Settings saved");
    expect(server.puts).toHaveLength(3);
    expect(server.values.enrichment_concurrency).toBe(6);
  });

  test("the conflict panel keeps the draft when leaving is attempted", async () => {
    server.stale.set(1, () => {
      server.values.enrichment_concurrency = 8;
      server.revision += 1;
    });
    const user = userEvent.setup();
    await loaded();
    await type(user, "Concurrent lookups", "6");
    await user.click(save());
    await screen.findByText("Settings changed while you were editing");
    await user.click(screen.getByRole("link", { name: /Covers/ }));
    expect(await screen.findByRole("alertdialog")).toBeInTheDocument();
  });
});
