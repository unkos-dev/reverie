import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

import { TypeaheadMultiSelect, type TypeaheadOption } from "./TypeaheadMultiSelect";

const LE_GUIN = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const LEM = "bbbbbbbb-bbbb-4bbb-9bbb-bbbbbbbbbbbb";

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockSuggest(suggestions: { id: string; value: string }[]): void {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ suggestions }));
}

function Harness({ initial = [] }: { initial?: TypeaheadOption[] }): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [selected, setSelected] = useState<TypeaheadOption[]>(initial);
  return (
    <QueryClientProvider client={client}>
      <TypeaheadMultiSelect
        kind="authors"
        label="author"
        selected={selected}
        onChange={setSelected}
      />
    </QueryClientProvider>
  );
}

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("TypeaheadMultiSelect", () => {
  test("gates the request below the minimum query length", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(jsonResponse({ suggestions: [] }));
    render(<Harness />);
    const user = userEvent.setup();

    await user.type(screen.getByRole("combobox", { name: /add author/i }), "l");

    expect(await screen.findByText(/at least 2 characters/i)).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 300));
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  test("fires the suggest request once the debounce settles at 2+ characters", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(
        jsonResponse({ suggestions: [{ id: LE_GUIN, value: "Ursula K. Le Guin" }] }),
      );
    render(<Harness />);
    const user = userEvent.setup();

    await user.type(screen.getByRole("combobox", { name: /add author/i }), "le");

    await waitFor(() => {
      expect(fetchSpy).toHaveBeenCalled();
    });
    const url = fetchSpy.mock.calls[0]?.[0] as URL;
    expect(url.pathname).toBe("/api/v1/authors/suggest");
    expect(url.searchParams.get("q")).toBe("le");
  });

  test("clicking a suggestion adds a chip and clears the input", async () => {
    mockSuggest([{ id: LE_GUIN, value: "Ursula K. Le Guin" }]);
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    const option = await screen.findByRole("option", { name: "Ursula K. Le Guin" });
    await user.click(option);

    const chips = screen.getByRole("list", { name: /selected author/i });
    expect(within(chips).getByRole("button", { name: /Remove Ursula K\. Le Guin/i })).toBeVisible();
    expect(input).toHaveValue("");
  });

  test("a committed option is no longer offered when the query is retyped", async () => {
    mockSuggest([
      { id: LE_GUIN, value: "Ursula K. Le Guin" },
      { id: LEM, value: "Stanislaw Lem" },
    ]);
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    await user.click(await screen.findByRole("option", { name: "Ursula K. Le Guin" }));

    await user.type(input, "le");
    await waitFor(() => {
      expect(screen.getByRole("option", { name: "Stanislaw Lem" })).toBeInTheDocument();
    });
    expect(screen.queryByRole("option", { name: "Ursula K. Le Guin" })).not.toBeInTheDocument();
  });

  test("removes a chip via its dismiss button", async () => {
    render(<Harness initial={[{ id: LE_GUIN, value: "Ursula K. Le Guin" }]} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i }));

    expect(
      screen.queryByRole("button", { name: /Remove Ursula K\. Le Guin/i }),
    ).not.toBeInTheDocument();
  });

  test("Backspace on an empty input removes the last chip", async () => {
    render(
      <Harness
        initial={[
          { id: LE_GUIN, value: "Ursula K. Le Guin" },
          { id: LEM, value: "Stanislaw Lem" },
        ]}
      />,
    );
    const user = userEvent.setup();

    const input = screen.getByRole("combobox", { name: /add author/i });
    input.focus();
    await user.keyboard("{Backspace}");

    expect(screen.queryByRole("button", { name: /Remove Stanislaw Lem/i })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Remove Ursula K\. Le Guin/i })).toBeInTheDocument();
  });

  test("commits the highlighted option on Enter and clears the input", async () => {
    mockSuggest([{ id: LE_GUIN, value: "Ursula K. Le Guin" }]);
    render(<Harness />);
    const user = userEvent.setup();
    const input = screen.getByRole("combobox", { name: /add author/i });

    await user.type(input, "le");
    await screen.findByRole("option", { name: "Ursula K. Le Guin" });
    await user.keyboard("{Enter}");

    expect(
      await screen.findByRole("button", { name: /Remove Ursula K\. Le Guin/i }),
    ).toBeInTheDocument();
    expect(input).toHaveValue("");
  });
});
