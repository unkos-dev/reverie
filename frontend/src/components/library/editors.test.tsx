import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { emptyFilterState, type FilterState, type SetFilter } from "@/routes/library-params";

import {
  StatusEditor,
  TextFilterEditor,
  VocabEditor,
  type TextOp,
  type VocabFamily,
} from "./editors";

const TEXT_OPS: readonly TextOp[] = ["contains", "eq", "ne"];

describe("TextFilterEditor", () => {
  test("typing the value reports the active operator", () => {
    const onChange = vi.fn();
    render(<TextFilterEditor value={{}} ops={TEXT_OPS} onChange={onChange} />);

    fireEvent.change(screen.getByRole("textbox", { name: "Filter value" }), {
      target: { value: "dune" },
    });

    expect(onChange).toHaveBeenLastCalledWith({ op: "contains", text: "dune" });
  });

  test("only the allowed operators are offered", async () => {
    render(<TextFilterEditor value={{}} ops={["contains", "empty"]} onChange={vi.fn()} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /operator/i }));

    expect(screen.getByRole("option", { name: "contains" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "is empty" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "equals" })).not.toBeInTheDocument();
  });

  test("switching to `is empty` reports empty and hides the value input", async () => {
    const onChange = vi.fn();
    function Harness(): ReactElement {
      const [value, setValue] = useState({});
      return (
        <TextFilterEditor
          value={value}
          ops={["contains", "empty"]}
          onChange={(next) => {
            onChange(next);
            setValue(next);
          }}
        />
      );
    }
    render(<Harness />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /operator/i }));
    await user.click(screen.getByRole("option", { name: "is empty" }));

    expect(onChange).toHaveBeenLastCalledWith({ op: "empty", text: "" });
    expect(screen.queryByRole("textbox", { name: "Filter value" })).not.toBeInTheDocument();
  });
});

describe("StatusEditor", () => {
  test("checking a status adds its token", async () => {
    const onChange = vi.fn();
    render(<StatusEditor value={[]} onChange={onChange} />);
    const user = userEvent.setup();

    await user.click(screen.getByLabelText("Unread"));

    expect(onChange).toHaveBeenLastCalledWith(["unread"]);
  });

  test("unchecking a status removes its token", async () => {
    const onChange = vi.fn();
    render(<StatusEditor value={["unread", "reading"]} onChange={onChange} />);
    const user = userEvent.setup();

    await user.click(screen.getByLabelText("Unread"));

    expect(onChange).toHaveBeenLastCalledWith(["reading"]);
  });
});

describe("editor id scoping", () => {
  test("two named status groups keep their native labels and changes independent", async () => {
    const firstChange = vi.fn();
    const secondChange = vi.fn();
    render(
      <>
        <StatusEditor value={[]} onChange={firstChange} />
        <StatusEditor value={["reading"]} onChange={secondChange} />
      </>,
    );
    const user = userEvent.setup();
    const [first, second] = screen.getAllByRole("group", { name: "Reading status" });
    await user.click(within(first).getByText("Unread"));
    expect(firstChange).toHaveBeenCalledExactlyOnceWith(["unread"]);
    expect(secondChange).not.toHaveBeenCalled();
    const reading = within(second).getByRole("checkbox", { name: "Reading" });
    reading.focus();
    await user.keyboard(" ");
    expect(secondChange).toHaveBeenCalledExactlyOnceWith([]);
    expect(firstChange).toHaveBeenCalledTimes(1);
    expect(reading).toHaveFocus();
  });
});

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockSuggest(suggestions: { id: string | null; value: string }[]): void {
  vi.spyOn(globalThis, "fetch").mockImplementation(() =>
    Promise.resolve(jsonResponse({ suggestions })),
  );
}

function VocabHarness({
  initial,
  family = "tags",
}: {
  initial: SetFilter;
  family?: VocabFamily;
}): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [draft, setDraft] = useState<FilterState>({ ...emptyFilterState(), [family]: initial });
  return (
    <QueryClientProvider client={client}>
      <VocabEditor
        family={family}
        draft={draft}
        setDraft={setDraft}
        resolveAuthorLabel={(id) => id}
      />
      <output data-testid="vocab-draft">{JSON.stringify(draft[family])}</output>
    </QueryClientProvider>
  );
}

describe("VocabEditor", () => {
  beforeEach(() => {
    // Seed (not reset): apiFetch lazily hydrates an empty cache with a
    // leading /auth/me fetch that would eat these suites' response mocks.
    __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
    vi.restoreAllMocks();
  });

  test("mode switch preserves per-mode lists", async () => {
    render(<VocabHarness initial={{ all: ["fiction"], any: ["mystery"], none: ["romance"] }} />);
    const user = userEvent.setup();

    // any.length > 0 wins the initial mode.
    expect(screen.getByRole("button", { name: /remove mystery/i })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "all of" }));
    expect(screen.getByRole("button", { name: /remove fiction/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /remove mystery/i })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "none of" }));
    expect(screen.getByRole("button", { name: /remove romance/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /remove fiction/i })).not.toBeInTheDocument();
  });

  test("adding a token that exists in another mode removes it from that other mode", async () => {
    mockSuggest([{ id: null, value: "mystery" }]);
    render(<VocabHarness initial={{ all: [], any: ["mystery"], none: [] }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "all of" }));

    const input = screen.getByRole("combobox", { name: /add tags/i });
    await user.type(input, "my");
    await user.click(await screen.findByRole("option", { name: "mystery" }));
    await user.keyboard("{Escape}");

    expect(screen.getByRole("button", { name: /remove mystery/i })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "any of" }));
    expect(screen.queryByRole("button", { name: /remove mystery/i })).not.toBeInTheDocument();
  });

  test("ordinary add and remove round-trips", async () => {
    mockSuggest([{ id: null, value: "fantasy" }]);
    render(<VocabHarness initial={{ all: [], any: [], none: [] }} />);
    const user = userEvent.setup();

    const input = screen.getByRole("combobox", { name: /add tags/i });
    await user.type(input, "fa");
    await user.click(await screen.findByRole("option", { name: "fantasy" }));

    await user.keyboard("{Escape}");
    const chip = screen.getByRole("button", { name: /remove fantasy/i });
    expect(chip).toBeInTheDocument();

    await user.click(chip);
    expect(screen.queryByRole("button", { name: /remove fantasy/i })).not.toBeInTheDocument();
  });
});

test.each(["authors", "tags", "genres", "moods"] as const)(
  "%s commits its persisted identity and keeps mode sets exclusive",
  async (family) => {
    __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
    const id = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    const value = "Literary";
    const persisted = family === "authors" ? id : value;
    mockSuggest([{ id, value }]);
    render(<VocabHarness family={family} initial={{ all: [], any: [persisted], none: [] }} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Match mode/ }));
    await user.click(screen.getByRole("option", { name: "none of" }));
    await user.type(screen.getByRole("combobox"), "li");
    await user.click(await screen.findByRole("option", { name: value }));
    await user.keyboard("{Escape}");
    expect(JSON.parse(screen.getByTestId("vocab-draft").textContent)).toEqual({
      all: [],
      any: [],
      none: [persisted],
    });
  },
);
