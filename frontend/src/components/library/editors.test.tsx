import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { emptyFilterState, type FilterState, type SetFilter } from "@/routes/library-params";

import {
  DateRangeEditor,
  RangeFilterEditor,
  StatusEditor,
  TextFilterEditor,
  VocabEditor,
  type TextOp,
} from "./editors";

const TEXT_OPS: readonly TextOp[] = ["contains", "eq", "ne"];

describe("TextFilterEditor", () => {
  test("typing the value reports the active operator", () => {
    const onChange = vi.fn();
    render(<TextFilterEditor value={{}} ops={TEXT_OPS} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Value"), { target: { value: "dune" } });

    expect(onChange).toHaveBeenLastCalledWith({ contains: "dune" });
  });

  test("only the allowed operators are offered", async () => {
    render(<TextFilterEditor value={{}} ops={["contains", "empty"]} onChange={vi.fn()} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("combobox", { name: /operator/i }));

    expect(screen.getByRole("option", { name: "contains" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "is empty" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "equals" })).not.toBeInTheDocument();
  });

  test("switching to `is empty` reports empty and hides the value input", async () => {
    const onChange = vi.fn();
    render(<TextFilterEditor value={{}} ops={["contains", "empty"]} onChange={onChange} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("combobox", { name: /operator/i }));
    await user.click(screen.getByRole("option", { name: "is empty" }));

    expect(onChange).toHaveBeenLastCalledWith({ empty: true });
    expect(screen.queryByLabelText("Value")).not.toBeInTheDocument();
  });
});

describe("RangeFilterEditor", () => {
  test("typing a min reports the lower bound, keeping the upper", () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{ lte: 500 }} min={0} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Min"), { target: { value: "100" } });

    expect(onChange).toHaveBeenLastCalledWith({ lte: 500, gte: 100 });
  });

  test("linked bounds expose the peer and intrinsic limits", () => {
    render(
      <RangeFilterEditor value={{ gte: 300, lte: 700 }} min={0} max={1000} onChange={vi.fn()} />,
    );

    expect(screen.getByLabelText("Min")).toHaveAttribute("min", "0");
    expect(screen.getByLabelText("Min")).toHaveAttribute("max", "700");
    expect(screen.getByLabelText("Max")).toHaveAttribute("min", "300");
    expect(screen.getByLabelText("Max")).toHaveAttribute("max", "1000");
  });

  test("equal and one-sided bounds keep legal native ranges", () => {
    const { rerender } = render(
      <RangeFilterEditor value={{ gte: 3, lte: 3 }} min={1} max={5} onChange={vi.fn()} />,
    );

    expect(screen.getByLabelText("Min")).toHaveAttribute("min", "1");
    expect(screen.getByLabelText("Min")).toHaveAttribute("max", "3");
    expect(screen.getByLabelText("Max")).toHaveAttribute("min", "3");
    expect(screen.getByLabelText("Max")).toHaveAttribute("max", "5");

    rerender(<RangeFilterEditor value={{}} min={1} max={5} onChange={vi.fn()} />);
    expect(screen.getByLabelText("Min")).toHaveAttribute("min", "1");
    expect(screen.getByLabelText("Min")).toHaveAttribute("max", "5");
    expect(screen.getByLabelText("Max")).toHaveAttribute("min", "1");
    expect(screen.getByLabelText("Max")).toHaveAttribute("max", "5");
  });

  test("a crossing max stays local while typing 9, 90, then publishes 900", () => {
    const onChange = vi.fn();
    const { rerender } = render(<RangeFilterEditor value={{ gte: 300 }} onChange={onChange} />);
    const max = screen.getByLabelText("Max");

    fireEvent.change(max, { target: { value: "9" } });
    fireEvent.change(max, { target: { value: "90" } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(max, { target: { value: "900" } });

    expect(onChange).toHaveBeenLastCalledWith({ gte: 300, lte: 900 });
    expect(onChange).toHaveBeenCalledTimes(1);
    rerender(<RangeFilterEditor value={{ gte: 300, lte: 900 }} onChange={onChange} />);
    expect(max).toHaveValue(900);
  });

  test("blur clamps a crossing edit without changing its peer", () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{ gte: 300, lte: 700 }} onChange={onChange} />);

    const max = screen.getByLabelText("Max");
    fireEvent.change(max, { target: { value: "90" } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.blur(max);
    expect(onChange).toHaveBeenLastCalledWith({ gte: 300, lte: 300 });

    const min = screen.getByLabelText("Min");
    fireEvent.change(min, { target: { value: "900" } });
    fireEvent.blur(min);
    expect(onChange).toHaveBeenLastCalledWith({ gte: 700, lte: 700 });
  });

  test("a malformed draft preserves the prior bound until blur, then blank clears it", () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{ gte: 300 }} onChange={onChange} />);
    const min = screen.getByLabelText("Min");

    fireEvent.change(min, { target: { value: "12.5" } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.blur(min);
    expect(min).toHaveValue(300);

    fireEvent.change(min, { target: { value: "" } });
    expect(onChange).toHaveBeenLastCalledWith({ gte: undefined });
  });

  test("native bad input preserves the prior bound until blur", () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{ gte: 300 }} onChange={onChange} />);
    const min = screen.getByLabelText("Min");

    Object.defineProperty(min, "validity", { configurable: true, value: { badInput: true } });
    fireEvent.change(min, { target: { value: "" } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.blur(min);

    expect(min).toHaveValue(300);
    expect(onChange).not.toHaveBeenCalled();
  });

  test("blur clamps edits to intrinsic numeric limits", () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{}} min={1} max={5} onChange={onChange} />);

    const min = screen.getByLabelText("Min");
    fireEvent.change(min, { target: { value: "0" } });
    fireEvent.blur(min);
    expect(onChange).toHaveBeenLastCalledWith({ gte: 1 });

    const max = screen.getByLabelText("Max");
    fireEvent.change(max, { target: { value: "6" } });
    fireEvent.blur(max);
    expect(onChange).toHaveBeenLastCalledWith({ lte: 5 });
  });

  test("external clears and disabled state reset local drafts", () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <RangeFilterEditor value={{ gte: 300 }} allowEmpty onChange={onChange} />,
    );
    const min = screen.getByLabelText("Min");

    fireEvent.change(min, { target: { value: "12.5" } });
    rerender(<RangeFilterEditor value={{}} allowEmpty onChange={onChange} />);
    expect(min).toHaveValue(null);

    rerender(
      <RangeFilterEditor value={{ gte: 300, empty: true }} allowEmpty onChange={onChange} />,
    );
    expect(screen.getByLabelText("Min")).toBeDisabled();
    expect(screen.getByLabelText("Min")).toHaveValue(null);
  });

  test("Escape resets the local draft and still bubbles to the parent", () => {
    const parentKeyDown = vi.fn();
    render(
      <div onKeyDown={parentKeyDown}>
        <RangeFilterEditor value={{ gte: 300 }} onChange={vi.fn()} />
      </div>,
    );
    const min = screen.getByLabelText("Min");

    fireEvent.change(min, { target: { value: "12.5" } });
    fireEvent.keyDown(min, { key: "Escape" });

    expect(min).toHaveValue(300);
    expect(parentKeyDown).toHaveBeenCalled();
  });

  test("checking `has no value` reports empty and disables the inputs", async () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{ gte: 3 }} allowEmpty onChange={onChange} />);
    const user = userEvent.setup();

    await user.click(screen.getByLabelText("Has no value"));

    expect(onChange).toHaveBeenLastCalledWith({ empty: true });
  });

  test("no empty toggle unless allowed", () => {
    render(<RangeFilterEditor value={{}} onChange={vi.fn()} />);
    expect(screen.queryByLabelText("Has no value")).not.toBeInTheDocument();
  });
});

describe("DateRangeEditor", () => {
  test("changing the after bound reports it, keeping before", () => {
    const onChange = vi.fn();
    render(<DateRangeEditor before="2026-06-30" onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("After"), { target: { value: "2026-01-01" } });

    expect(onChange).toHaveBeenLastCalledWith({ after: "2026-01-01", before: "2026-06-30" });
  });

  test("date bounds expose peer limits and clamp a typed crossing edit", () => {
    const onChange = vi.fn();
    render(<DateRangeEditor after="2026-01-01" before="2026-06-30" onChange={onChange} />);

    const after = screen.getByLabelText("After");
    const before = screen.getByLabelText("Before");
    expect(after).toHaveAttribute("max", "2026-06-30");
    expect(before).toHaveAttribute("min", "2026-01-01");

    fireEvent.change(after, { target: { value: "2026-12-01" } });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.keyDown(after, { key: "Enter" });
    expect(onChange).toHaveBeenLastCalledWith({ after: "2026-06-30", before: "2026-06-30" });

    fireEvent.change(before, { target: { value: "2025-01-01" } });
    fireEvent.blur(before);
    expect(onChange).toHaveBeenLastCalledWith({ after: "2026-01-01", before: "2026-01-01" });
  });

  test("blank date bounds clear the edited side", () => {
    const onChange = vi.fn();
    render(<DateRangeEditor after="2026-01-01" before="2026-06-30" onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Before"), { target: { value: "" } });

    expect(onChange).toHaveBeenLastCalledWith({ after: "2026-01-01", before: undefined });
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
  test("two RangeFilterEditor instances render disjoint input ids", () => {
    render(
      <>
        <RangeFilterEditor value={{}} allowEmpty onChange={vi.fn()} />
        <RangeFilterEditor value={{}} allowEmpty onChange={vi.fn()} />
      </>,
    );

    const [firstMin, secondMin] = screen.getAllByLabelText("Min");
    const [firstMinLabel, secondMinLabel] = screen.getAllByText("Min");
    expect(firstMin.id).not.toBe(secondMin.id);
    expect(firstMinLabel.getAttribute("for")).toBe(firstMin.id);
    expect(secondMinLabel.getAttribute("for")).toBe(secondMin.id);
  });

  test("two StatusEditor instances render disjoint checkbox ids", () => {
    render(
      <>
        <StatusEditor value={[]} onChange={vi.fn()} />
        <StatusEditor value={[]} onChange={vi.fn()} />
      </>,
    );

    const [firstUnread, secondUnread] = screen.getAllByLabelText("Unread");
    const [firstUnreadLabel, secondUnreadLabel] = screen.getAllByText("Unread");
    expect(firstUnread.id).not.toBe(secondUnread.id);
    expect(firstUnreadLabel.getAttribute("for")).toBe(firstUnread.id);
    expect(secondUnreadLabel.getAttribute("for")).toBe(secondUnread.id);
  });
});

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
}

function mockSuggest(suggestions: { id: string | null; value: string }[]): void {
  vi.spyOn(globalThis, "fetch").mockResolvedValue(jsonResponse({ suggestions }));
}

function VocabHarness({ initial }: { initial: SetFilter }): ReactElement {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: { queries: { retry: false } } }),
  );
  const [draft, setDraft] = useState<FilterState>({ ...emptyFilterState(), tags: initial });
  return (
    <QueryClientProvider client={client}>
      <VocabEditor
        family="tags"
        draft={draft}
        setDraft={setDraft}
        resolveAuthorLabel={(id) => id}
      />
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

    await user.click(screen.getByRole("combobox", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "all of" }));
    expect(screen.getByRole("button", { name: /remove fiction/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /remove mystery/i })).not.toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "none of" }));
    expect(screen.getByRole("button", { name: /remove romance/i })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /remove fiction/i })).not.toBeInTheDocument();
  });

  test("adding a token that exists in another mode removes it from that other mode", async () => {
    mockSuggest([{ id: null, value: "mystery" }]);
    render(<VocabHarness initial={{ all: [], any: ["mystery"], none: [] }} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("combobox", { name: /match mode/i }));
    await user.click(screen.getByRole("option", { name: "all of" }));

    const input = screen.getByRole("combobox", { name: /add tags/i });
    await user.type(input, "my");
    await user.click(await screen.findByRole("option", { name: "mystery" }));

    expect(screen.getByRole("button", { name: /remove mystery/i })).toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: /match mode/i }));
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

    const chip = screen.getByRole("button", { name: /remove fantasy/i });
    expect(chip).toBeInTheDocument();

    await user.click(chip);
    expect(screen.queryByRole("button", { name: /remove fantasy/i })).not.toBeInTheDocument();
  });
});
