import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import {
  createMemoryRouter,
  RouterProvider,
  Outlet,
  useHref,
  useNavigate,
  type RouteObject,
} from "react-router";
import { RouterProvider as AriaRouterProvider } from "react-aria-components";
import { useState, type ReactElement, type ReactNode } from "react";

import { openCommandPalette } from "@/lib/command-palette";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";

import { CommandPalette, HighlightedSnippet } from "@/components/CommandPalette";

async function triggerCmdK(): Promise<void> {
  await userEvent.keyboard("{Control>}k{/Control}");
}

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

type PaletteHarness = {
  router: ReturnType<typeof createMemoryRouter>;
};

async function renderPalette(
  initialEntries: string[] = ["/library"],
  parent?: ReactNode,
): Promise<PaletteHarness> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function Shell(): ReactElement {
    const navigate = useNavigate();
    return (
      <AriaRouterProvider
        navigate={(to, options) => {
          void navigate(to, options);
        }}
        useHref={useHref}
      >
        {parent}
        <CommandPalette />
        <Outlet />
      </AriaRouterProvider>
    );
  }
  const routes: RouteObject[] = [
    {
      element: <Shell />,
      children: [
        { path: "/library", element: <div /> },
        { path: "/b/:id", element: <div data-testid="book-route" /> },
      ],
    },
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
  await render(<Wrapper />);
  return { router };
}

beforeEach(() => {
  vi.restoreAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("CommandPalette — Cmd-K binding", () => {
  test("is closed by default", async () => {
    await renderPalette();
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  });

  test("opens on Ctrl+K and closes on Escape", async () => {
    await renderPalette();
    const user = userEvent;
    await triggerCmdK();
    await expect.element(page.getByRole("dialog", { name: /search library/i })).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    });
  });

  test.each(["ctrlKey", "metaKey"])(
    "%s+K closes a populated palette and reopening starts with an empty query",
    async (modifier) => {
      vi.spyOn(globalThis, "fetch").mockImplementation(() =>
        Promise.resolve(jsonResponse({ items: [] })),
      );
      await renderPalette();
      const user = userEvent;
      await userEvent.keyboard(
        modifier === "ctrlKey" ? "{Control>}k{/Control}" : "{Meta>}k{/Meta}",
      );
      const input = page.getByRole("searchbox", { name: "Search library" });
      await user.type(input, "word");
      await expect.element(input).toHaveValue("word");
      await userEvent.keyboard(
        modifier === "ctrlKey" ? "{Control>}k{/Control}" : "{Meta>}k{/Meta}",
      );
      await vi.waitFor(async () => {
        await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
      });
      await userEvent.keyboard(
        modifier === "ctrlKey" ? "{Control>}k{/Control}" : "{Meta>}k{/Meta}",
      );
      await expect.element(page.getByRole("searchbox", { name: "Search library" })).toHaveValue("");
    },
  );

  test("opens on '/' when not typing in a field", async () => {
    await renderPalette();
    await userEvent.keyboard("/");
    await expect.element(page.getByRole("dialog", { name: /search library/i })).toBeInTheDocument();
  });

  test("ignores '/' while a field is focused", async () => {
    await renderPalette();
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    await userEvent.keyboard("/");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    field.remove();
  });
});

describe("CommandPalette — module trigger (openCommandPalette)", () => {
  test("opens the dialog when invoked", async () => {
    await renderPalette();
    openCommandPalette();
    await expect.element(page.getByRole("dialog", { name: /search library/i })).toBeInTheDocument();
  });

  test("escape closes and focus returns to the previously focused element", async () => {
    await renderPalette();
    const user = userEvent;
    const invoker = document.createElement("button");
    invoker.textContent = "open search";
    document.body.appendChild(invoker);
    invoker.focus();
    openCommandPalette();
    await expect.element(page.getByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    });
    await vi.waitFor(async () => {
      await expect.element(invoker).toHaveFocus();
    });
    invoker.remove();
  });
});

describe("CommandPalette — dialog semantics (spec §6)", () => {
  test("dialog hides background content and exposes native menu results and keyboard hints", async () => {
    await renderPalette();
    await triggerCmdK();
    const dialog = page.getByRole("dialog", { name: /search library/i });
    await expect.element(dialog).toBeInTheDocument();
    await expect
      .element(dialog)
      .toHaveAttribute(
        "aria-labelledby",
        dialog.getByRole("heading", { name: "Search library" }).element().id,
      );
    expect(page.getByText("Background action").element().closest("[inert]")).not.toBeNull();
    await expect.element(page.getByRole("menu", { name: "Books" })).toBeInTheDocument();
    await expect.element(page.getByText(/navigate/i)).toBeInTheDocument();
    await expect.element(page.getByText(/close/i)).toBeInTheDocument();
  });
});

describe("CommandPalette — search flow", () => {
  test("Tab traverses native links and arrow keys navigate results with input focus", async () => {
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
    const { router } = await renderPalette();
    const user = userEvent;
    await triggerCmdK();
    const input = page.getByRole("searchbox", { name: "Search library" });
    await user.type(input, "williams");
    const first = page.getByRole("menuitem", { name: /Stoner/ });
    const second = page.getByRole("menuitem", { name: /Butcher's Crossing/ });
    await expect.element(input).toHaveFocus();
    await expect.element(input).toHaveAttribute("aria-activedescendant", first.element().id);
    await user.tab();
    await expect.element(first).toHaveFocus();
    await user.tab({ shift: true });
    await expect.element(input).toHaveFocus();
    await expect.element(input).toHaveAttribute("aria-activedescendant", first.element().id);
    await user.keyboard("{ArrowDown}");
    await expect.element(input).toHaveFocus();
    await expect.element(input).toHaveAttribute("aria-activedescendant", second.element().id);
    await user.keyboard("{ArrowUp}");
    await expect.element(input).toHaveFocus();
    await expect.element(input).toHaveAttribute("aria-activedescendant", first.element().id);
    await user.keyboard("{ArrowDown}{Enter}");
    await vi.waitFor(() => {
      expect(router.state.location.pathname).toBe("/b/book-1");
    });
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await triggerCmdK();
    await expect.element(page.getByRole("searchbox", { name: "Search library" })).toHaveValue("");
  });

  test("does not fire a request while query is under MIN_Q_LEN", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");
    await renderPalette();
    const user = userEvent;
    await triggerCmdK();
    const input = page.getByPlaceholder("Find a volume by title, author, quote…");
    await user.type(input, "g");
    // No call should fire — under MIN_Q_LEN (2).
    await new Promise((r) => setTimeout(r, 300));
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("starts eligible search without an added query delay", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockImplementation(() => Promise.resolve(jsonResponse({ items: [] })));
    await renderPalette();
    await triggerCmdK();
    const input = page.getByPlaceholder("Find a volume by title, author, quote…");
    await userEvent.fill(input, "war");
    expect(fetchSpy).toHaveBeenCalled();
    const url = fetchSpy.mock.calls[0]?.[0];
    if (!(url instanceof URL)) throw new Error("Search URL missing");
    expect(url.pathname).toBe("/api/v1/search");
    expect(url.searchParams.get("q")).toBe("war");
  });

  test("a result exposes an encoded link and native pointer navigation closes and resets the persistent palette", async () => {
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(
        jsonResponse({
          items: [
            {
              kind: "book",
              id: "book/with space",
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
    const { router } = await renderPalette();
    const user = userEvent;
    await triggerCmdK();
    const input = page.getByPlaceholder("Find a volume by title, author, quote…");
    await user.type(input, "stoner");
    const result = page.getByRole("menuitem", { name: /Stoner/ });
    await expect.element(result).toHaveAttribute("href", "/b/book%2Fwith%20space");
    await user.click(result);
    await vi.waitFor(() => {
      expect(router.state.location.pathname).toBe("/b/book%2Fwith%20space");
    });
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await triggerCmdK();
    await expect.element(page.getByRole("searchbox", { name: "Search library" })).toHaveValue("");
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
    await renderPalette();
    const user = userEvent;
    await triggerCmdK();
    const input = page.getByPlaceholder("Find a volume by title, author, quote…");
    await user.type(input, "stonre");
    await expect.element(page.getByRole("menuitem", { name: /Stoner/ })).toBeInTheDocument();
    await expect.element(page.getByText("No results.")).not.toBeInTheDocument();
  });

  test("logs and surfaces a user message on query error", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    vi.spyOn(globalThis, "fetch").mockImplementation(() =>
      Promise.resolve(new Response("upstream blew up", { status: 500 })),
    );
    await renderPalette();
    const user = userEvent;
    await triggerCmdK();
    const input = page.getByPlaceholder("Find a volume by title, author, quote…");
    await user.type(input, "war");
    await expect.element(page.getByText(/search failed/i)).toBeInTheDocument();
    expect(consoleSpy).toHaveBeenCalled();
  });
});

describe("HighlightedSnippet", () => {
  const STX = "\u0002";
  const ETX = "\u0003";

  test("plain text without markers renders as a single span (no mark)", async () => {
    await render(<HighlightedSnippet text="plain text" />);
    await expect.element(page.getByText("plain text")).toBeInTheDocument();
    expect(document.querySelector("mark")).toBeNull();
  });

  test("wraps text between STX and ETX in a <mark>", async () => {
    await render(<HighlightedSnippet text={`the ${STX}galaxy${ETX} awaits`} />);
    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(1);
    expect(marks[0].textContent).toBe("galaxy");
  });

  test("handles multiple highlighted runs without key collisions", async () => {
    await render(<HighlightedSnippet text={`${STX}war${ETX} and ${STX}peace${ETX}`} />);
    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(2);
    expect(marks[0].textContent).toBe("war");
    expect(marks[1].textContent).toBe("peace");
  });

  test("user-typography ‹› does not become a highlight (no collision)", async () => {
    // STX/ETX were chosen specifically because no valid text can carry
    // them — French guillemets must render literally with no <mark>.
    await render(<HighlightedSnippet text="quoted ‹like this›" />);
    expect(document.querySelectorAll("mark")).toHaveLength(0);
  });

  test("HTML in the snippet renders as literal text, not as markup", async () => {
    const { container } = await render(
      <HighlightedSnippet text={`${STX}<script>alert(1)</script>${ETX}`} />,
    );
    expect(container.querySelector("script")).toBeNull();
    expect(document.querySelector("mark")?.textContent).toBe("<script>alert(1)</script>");
  });

  test("does not crash on an unclosed start marker", async () => {
    await render(<HighlightedSnippet text={`before ${STX}unclosed`} />);
    await expect.element(page.getByText("unclosed", { exact: true })).toBeInTheDocument();
  });
});

function ParentEditor(): ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <Dialog isOpen={open} onOpenChange={setOpen}>
      <Button type="button">Open parent editor</Button>
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
  await renderPalette(["/library"], <ParentEditor />);
  const user = userEvent;
  const opener = page.getByRole("button", { name: "Open parent editor" });
  await user.click(opener);
  const parent = page.getByRole("dialog", { name: "Parent editor" });
  const field = parent.getByRole("textbox", { name: "Parent value" });
  await user.keyboard("{Control>}K{/Control}");
  const palette = page.getByRole("dialog", { name: "Search library" });
  const input = palette.getByPlaceholder("Find a volume by title, author, quote…");
  await user.type(input, "word");
  await user.keyboard("{Escape}");
  await expect.element(input).toHaveValue("");
  await expect.element(palette).toBeInTheDocument();
  await expect.element(parent).toBeInTheDocument();
  await user.keyboard("{Escape}");
  await expect.element(palette).not.toBeInTheDocument();
  await expect.element(parent).toBeInTheDocument();
  await user.type(field, "Continue editing");
  await expect.element(field).toHaveValue("Continue editing");
  await user.keyboard("{Escape}");
  await expect.element(parent).not.toBeInTheDocument();
  await expect.element(opener).toHaveFocus();
});

test("a delayed search cannot replace newer results", async () => {
  let resolveOld: ((response: Response) => void) | undefined;
  const old = new Promise<Response>((resolve) => {
    resolveOld = resolve;
  });
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    let raw: string;
    if (typeof input === "string") {
      raw = input;
    } else if (input instanceof URL) {
      raw = input.href;
    } else {
      raw = input.url;
    }
    return new URL(raw).searchParams.get("q") === "old"
      ? old
      : Promise.resolve(jsonResponse({ items: [] }));
  });
  await renderPalette();
  await triggerCmdK();
  const input = page.getByPlaceholder("Find a volume by title, author, quote…");
  await userEvent.fill(input, "old");
  await expect.element(page.getByText("Searching…")).toBeInTheDocument();
  await userEvent.fill(input, "new");
  await expect.element(page.getByText("No results.")).toBeInTheDocument();
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
  await expect.element(input).toHaveValue("new");
  await expect.element(page.getByRole("menuitem", { name: "Old result" })).not.toBeInTheDocument();
});
