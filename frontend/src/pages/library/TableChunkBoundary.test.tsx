import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

import { TableChunkBoundary } from "./TableChunkBoundary";

function Detonator({ armed }: { armed: boolean }): ReactElement {
  if (armed) throw new Error("chunk load failed");
  return <div data-testid="table-content">table content</div>;
}

function Harness({ onFallbackToGrid }: { onFallbackToGrid?: () => void }): ReactElement {
  const [armed, setArmed] = useState(true);
  return (
    <>
      <button
        type="button"
        onClick={() => {
          setArmed(false);
        }}
      >
        defuse
      </button>
      <TableChunkBoundary onFallbackToGrid={onFallbackToGrid ?? ((): void => undefined)}>
        <Detonator armed={armed} />
      </TableChunkBoundary>
    </>
  );
}

describe("TableChunkBoundary", () => {
  test("renders children while nothing throws", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(
      <TableChunkBoundary onFallbackToGrid={(): void => undefined}>
        <div data-testid="table-content">table content</div>
      </TableChunkBoundary>,
    );
    expect(screen.getByTestId("table-content")).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  test("a throwing child degrades to the inline alert with a breadcrumb, page stays alive", () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<Harness />);
    expect(screen.getByRole("alert")).toHaveTextContent("Couldn't load the table view.");
    expect(screen.getByRole("button", { name: "defuse" })).toBeInTheDocument();
    expect(spy.mock.calls.some((call) => String(call[0]).includes("[TableChunkBoundary]"))).toBe(
      true,
    );
  });

  test("Try again resets the boundary and re-renders recovered children", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "defuse" }));
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByTestId("table-content")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  test("Switch to grid view invokes the fallback callback", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const onFallbackToGrid = vi.fn();
    const user = userEvent.setup();
    render(<Harness onFallbackToGrid={onFallbackToGrid} />);
    await user.click(screen.getByRole("button", { name: "Switch to grid view" }));
    expect(onFallbackToGrid).toHaveBeenCalledTimes(1);
  });
});
