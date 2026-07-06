import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, test, vi } from "vite-plus/test";

import { StatusCellEditor } from "./StatusCellEditor";

describe("StatusCellEditor", () => {
  test("renders every reading status plus the empty option", () => {
    render(<StatusCellEditor value="reading" onCommit={vi.fn()} />);
    const select = screen.getByRole("combobox", { name: "Reading status" });
    const options = Array.from(select.querySelectorAll("option")).map((option) => option.value);
    expect(options).toEqual(["", "want_to_read", "reading", "on_hold", "finished", "abandoned"]);
  });

  test("changing the selection commits immediately with the chosen status", async () => {
    const onCommit = vi.fn();
    const user = userEvent.setup();
    render(<StatusCellEditor value="want_to_read" onCommit={onCommit} />);
    const select = screen.getByRole("combobox", { name: "Reading status" });

    await user.selectOptions(select, "finished");

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith("finished");
  });

  test("selecting the empty option commits null", async () => {
    const onCommit = vi.fn();
    const user = userEvent.setup();
    render(<StatusCellEditor value="reading" onCommit={onCommit} />);
    const select = screen.getByRole("combobox", { name: "Reading status" });

    await user.selectOptions(select, "-");

    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit).toHaveBeenCalledWith(null);
  });
});
