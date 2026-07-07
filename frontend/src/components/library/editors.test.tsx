import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vite-plus/test";

import {
  DateRangeEditor,
  RangeFilterEditor,
  StatusEditor,
  TextFilterEditor,
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

  test("a non-integer clears the bound", () => {
    const onChange = vi.fn();
    render(<RangeFilterEditor value={{}} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Min"), { target: { value: "12.5" } });

    expect(onChange).toHaveBeenLastCalledWith({ gte: undefined });
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
