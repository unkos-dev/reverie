import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { openCommandPalette } from "@/lib/command-palette";

import { CommandPalette, HighlightedSnippet } from "./CommandPalette";

/**
 * Trigger the global Cmd-K binding. userEvent.keyboard targets
 * `document.activeElement` which jsdom doesn't always route to a
 * `window`-attached listener; dispatching directly on `window` is the
 * reliable shape for testing window keydown handlers.
 */
function triggerCmdK(): void {
  act(() => {
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
  });
}

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

interface PaletteHarness {
  router: ReturnType<typeof createMemoryRouter>;
}

function renderPalette(initialEntries: string[] = ["/library"]): PaletteHarness {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const routes: RouteObject[] = [
    { path: "/library", element: <CommandPalette /> },
    { path: "/b/:id", element: <div data-testid="book-route" /> },
  ];
  const router = createMemoryRouter(routes, { initialEntries });
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return { router };
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("CommandPalette — Cmd-K binding", () => {
  test("is closed by default", () => {
    renderPalette();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  test("opens on Ctrl+K and closes on Escape", async () => {
    renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    expect(await screen.findByRole("dialog", { name: /search library/i })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  test("opens on '/' when not typing in a field", async () => {
    renderPalette();
    act(() => {
      fireEvent.keyDown(window, { key: "/" });
    });
    expect(await screen.findByRole("dialog", { name: /search library/i })).toBeInTheDocument();
  });

  test("ignores '/' while a field is focused", () => {
    renderPalette();
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    act(() => {
      fireEvent.keyDown(field, { key: "/" });
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    field.remove();
  });
});

describe("CommandPalette — module trigger (openCommandPalette)", () => {
  test("opens the dialog when invoked", async () => {
    renderPalette();
    act(() => {
      openCommandPalette();
    });
    expect(await screen.findByRole("dialog", { name: /search library/i })).toBeInTheDocument();
  });

  test("escape closes and focus returns to the previously focused element", async () => {
    renderPalette();
    const user = userEvent.setup();
    const invoker = document.createElement("button");
    invoker.textContent = "open search";
    document.body.appendChild(invoker);
    invoker.focus();
    act(() => {
      openCommandPalette();
    });
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    await waitFor(() => {
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
    await waitFor(() => {
      expect(invoker).toHaveFocus();
    });
    invoker.remove();
  });
});

describe("CommandPalette — dialog semantics (spec §6)", () => {
  test("dialog is modal with listbox results semantics and kbd footer", async () => {
    renderPalette();
    triggerCmdK();
    const dialog = await screen.findByRole("dialog", { name: /search library/i });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(screen.getByText(/navigate/i)).toBeInTheDocument();
    expect(screen.getByText(/close/i)).toBeInTheDocument();
  });
});

describe("CommandPalette — search flow", () => {
  test("does not fire a request while query is under MIN_Q_LEN", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
    await user.type(input, "g");
    // No call should fire — under MIN_Q_LEN (2).
    await new Promise((r) => setTimeout(r, 300));
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("fires once the debounce settles on query ≥ MIN_Q_LEN", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ items: [] }));
    renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
    await user.type(input, "war");
    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalled();
    });
    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/search");
    expect(url.searchParams.get("q")).toBe("war");
  });

  test("clicking a result navigates to /b/{id}", async () => {
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      jsonResponse({
        items: [
          {
            kind: "book",
            id: "abc-123",
            work_id: "w-1",
            title: "Stoner",
            authors: ["John Williams"],
            snippet: null,
            cover_url: null,
          },
        ],
      }),
    );
    const { router } = renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
    await user.type(input, "stoner");
    await screen.findByText("Stoner");
    await user.click(screen.getByText("Stoner"));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/b/abc-123");
    });
  });

  test("logs and surfaces a user message on query error", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("upstream blew up", { status: 500 }),
    );
    renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
    await user.type(input, "war");
    await screen.findByText(/search failed/i);
    expect(consoleSpy).toHaveBeenCalled();
  });
});

describe("HighlightedSnippet", () => {
  const STX = "\u0002";
  const ETX = "\u0003";

  test("plain text without markers renders as a single span (no mark)", () => {
    render(<HighlightedSnippet text="plain text" />);
    expect(screen.queryByText("plain text")).toBeInTheDocument();
    expect(document.querySelector("mark")).toBeNull();
  });

  test("wraps text between STX and ETX in a <mark>", () => {
    render(<HighlightedSnippet text={`the ${STX}galaxy${ETX} awaits`} />);
    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(1);
    expect(marks[0].textContent).toBe("galaxy");
  });

  test("handles multiple highlighted runs without key collisions", () => {
    render(<HighlightedSnippet text={`${STX}war${ETX} and ${STX}peace${ETX}`} />);
    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(2);
    expect(marks[0].textContent).toBe("war");
    expect(marks[1].textContent).toBe("peace");
  });

  test("user-typography ‹› does not become a highlight (no collision)", () => {
    // STX/ETX were chosen specifically because no valid text can carry
    // them — French guillemets must render literally with no <mark>.
    render(<HighlightedSnippet text="quoted ‹like this›" />);
    expect(document.querySelectorAll("mark")).toHaveLength(0);
  });

  test("HTML in the snippet renders as literal text, not as markup", () => {
    render(<HighlightedSnippet text={`${STX}<script>alert(1)</script>${ETX}`} />);
    expect(document.querySelector("script")).toBeNull();
    expect(document.querySelector("mark")?.textContent).toBe("<script>alert(1)</script>");
  });

  test("does not crash on an unclosed start marker", () => {
    expect(() => render(<HighlightedSnippet text={`before ${STX}unclosed`} />)).not.toThrow();
  });
});
