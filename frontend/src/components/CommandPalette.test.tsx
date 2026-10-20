import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider, type RouteObject } from "react-router";
import { useState, type ReactElement, type ReactNode } from "react";

import { openCommandPalette } from "@/lib/command-palette";

import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";

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

function renderPalette(
  initialEntries: string[] = ["/library"],
  parent?: ReactNode,
): PaletteHarness {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const routes: RouteObject[] = [
    {
      path: "/library",
      element: (
        <>
          {parent}
          <CommandPalette />
        </>
      ),
    },
    { path: "/b/:id", element: <div data-testid="book-route" /> },
  ];
  const router = createMemoryRouter(routes, { initialEntries });
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <button type="button">Background action</button>
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
  test("dialog hides background content and exposes native menu results and keyboard hints", async () => {
    renderPalette();
    triggerCmdK();
    const dialog = await screen.findByRole("dialog", { name: /search library/i });
    expect(dialog).toBeInTheDocument();
    expect(dialog).toHaveAttribute(
      "aria-labelledby",
      within(dialog).getByRole("heading", { name: "Search library" }).id,
    );
    expect(
      screen
        .getByRole("button", { name: "Background action", hidden: true })
        .closest('[aria-hidden="true"]'),
    ).not.toBeNull();
    expect(screen.getByRole("menu", { name: "Books" })).toBeInTheDocument();
    expect(screen.getByText(/navigate/i)).toBeInTheDocument();
    expect(screen.getByText(/close/i)).toBeInTheDocument();
  });
});

describe("CommandPalette — search flow", () => {
  test("Tab stays in the search field and arrow keys navigate results with input focus", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(
        jsonResponse({
          items: ["Stoner", "Butcher's Crossing"].map((title, index) => ({
            kind: "book",
            id: `book-${String(index)}`,
            work_id: `work-${String(index)}`,
            title,
            authors: ["John Williams"],
            snippet: null,
            cover_url: null,
          })),
        }),
      ),
    );
    const { router } = renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    const input = await screen.findByRole("searchbox", { name: "Search library" });
    await user.type(input, "williams");
    const first = await screen.findByRole("menuitem", { name: /Stoner/ });
    const second = screen.getByRole("menuitem", { name: /Butcher's Crossing/ });
    expect(input).toHaveFocus();
    await user.tab();
    expect(input).toHaveFocus();
    await user.tab({ shift: true });
    expect(input).toHaveFocus();
    expect(input).toHaveAttribute("aria-activedescendant", first.id);
    await user.keyboard("{ArrowDown}");
    expect(input).toHaveFocus();
    expect(input).toHaveAttribute("aria-activedescendant", second.id);
    await user.keyboard("{ArrowUp}");
    expect(input).toHaveAttribute("aria-activedescendant", first.id);
    await user.keyboard("{ArrowDown}{Enter}");
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/b/book-1");
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

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

  test("starts eligible search without an added query delay", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => Promise.resolve(jsonResponse({ items: [] })));
    renderPalette();
    triggerCmdK();
    const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
    fireEvent.change(input, { target: { value: "war" } });
    expect(fetchSpy).toHaveBeenCalled();
    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/search");
    expect(url.searchParams.get("q")).toBe("war");
  });

  test("clicking a result navigates to /b/{id}", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(
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
      ),
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

  test("shows server results whose title does not contain the query", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(
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
      ),
    );
    renderPalette();
    const user = userEvent.setup();
    triggerCmdK();
    const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
    await user.type(input, "stonre");
    expect(await screen.findByRole("menuitem", { name: /Stoner/ })).toBeInTheDocument();
    expect(screen.queryByText("No results.")).not.toBeInTheDocument();
  });

  test("logs and surfaces a user message on query error", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(new Response("upstream blew up", { status: 500 })),
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

function ParentEditor(): ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <Dialog isOpen={open} onOpenChange={setOpen}>
      <DialogTrigger>
        <button type="button">Open parent editor</button>
      </DialogTrigger>
      <DialogContent>
        <DialogTitle>Parent editor</DialogTitle>
        <input aria-label="Parent value" />
      </DialogContent>
    </Dialog>
  );
}

test("native Escape clears search, then closes only the palette over an editor", async () => {
  vi.spyOn(globalThis, "fetch").mockImplementation(() =>
    Promise.resolve(jsonResponse({ items: [] })),
  );
  renderPalette(["/library"], <ParentEditor />);
  const user = userEvent.setup();
  const opener = screen.getByRole("button", { name: "Open parent editor" });
  await user.click(opener);
  const parent = await screen.findByRole("dialog", { name: "Parent editor" });
  const field = within(parent).getByRole("textbox", { name: "Parent value" });
  await user.click(field);
  await user.keyboard("{Control>}K{/Control}");
  const palette = await screen.findByRole("dialog", { name: "Search library" });
  const input = within(palette).getByPlaceholderText("Find a volume by title, author, quote…");
  await user.type(input, "word");
  await user.keyboard("{Escape}");
  expect(input).toHaveValue("");
  expect(palette).toBeInTheDocument();
  expect(parent).toBeInTheDocument();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(palette).not.toBeInTheDocument());
  expect(parent).toBeInTheDocument();
  await waitFor(() => expect(field).toHaveFocus());
  await user.keyboard("{Escape}");
  await waitFor(() => expect(parent).not.toBeInTheDocument());
  await waitFor(() => expect(opener).toHaveFocus());
});

test("a delayed search cannot replace newer results", async () => {
  let resolveOld: ((response: Response) => void) | undefined;
  const old = new Promise<Response>((resolve) => {
    resolveOld = resolve;
  });
  vi.spyOn(globalThis, "fetch").mockImplementation((input) =>
    new URL(
      typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
    ).searchParams.get("q") === "old"
      ? old
      : Promise.resolve(jsonResponse({ items: [] })),
  );
  renderPalette();
  triggerCmdK();
  const input = await screen.findByPlaceholderText("Find a volume by title, author, quote…");
  fireEvent.change(input, { target: { value: "old" } });
  expect(await screen.findByText("Searching…")).toBeInTheDocument();
  fireEvent.change(input, { target: { value: "new" } });
  expect(await screen.findByText("No results.")).toBeInTheDocument();
  await act(async () => {
    resolveOld?.(
      jsonResponse({
        items: [
          {
            kind: "book",
            id: "old-book",
            work_id: "old-work",
            title: "Old result",
            authors: [],
            snippet: null,
            cover_url: null,
          },
        ],
      }),
    );
    await old;
  });
  expect(input).toHaveValue("new");
  expect(screen.queryByRole("menuitem", { name: "Old result" })).not.toBeInTheDocument();
});
