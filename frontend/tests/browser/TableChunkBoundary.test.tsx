import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { describe, expect, test, vi } from "vite-plus/test";

import { TableChunkBoundary } from "@/pages/library/TableChunkBoundary";

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
  test("renders children while nothing throws", async () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    await render(
      <TableChunkBoundary onFallbackToGrid={(): void => undefined}>
        <div data-testid="table-content">table content</div>
      </TableChunkBoundary>,
    );
    await expect.element(page.getByTestId("table-content")).toBeInTheDocument();
    expect(spy).not.toHaveBeenCalled();
  });

  test("a throwing child degrades to the inline alert with a breadcrumb, page stays alive", async () => {
    const spy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    await render(<Harness />);
    await expect
      .element(page.getByRole("alert"))
      .toHaveTextContent("Couldn't load the table view.");
    await expect.element(page.getByRole("button", { name: "defuse" })).toBeInTheDocument();
    expect(spy.mock.calls.some((call) => String(call[0]).includes("[TableChunkBoundary]"))).toBe(
      true,
    );
  });

  test("a parent update leaves the failure and explicit reload action available", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const user = userEvent;
    await render(<Harness />);
    await user.click(page.getByRole("button", { name: "defuse" }));
    await expect
      .element(page.getByRole("alert"))
      .toHaveTextContent("Couldn't load the table view.");
    await expect.element(page.getByRole("button", { name: "Reload page" })).toBeEnabled();
    await expect.element(page.getByTestId("table-content")).not.toBeInTheDocument();
  });

  test("Switch to grid view invokes the fallback callback", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const onFallbackToGrid = vi.fn();
    const user = userEvent;
    await render(<Harness onFallbackToGrid={onFallbackToGrid} />);
    await user.click(page.getByRole("button", { name: "Switch to grid view" }));
    expect(onFallbackToGrid).toHaveBeenCalledTimes(1);
  });
});
