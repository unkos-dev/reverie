import { render, screen, waitFor } from "@testing-library/react";
import { useEffect, type ReactElement } from "react";
import { createMemoryRouter, RouterProvider, useLocation } from "react-router";
import { describe, expect, test } from "vite-plus/test";

import { useLiveSearchParams, type ApplyParams } from "./use-live-search-params";

function renderHost(initialEntry: string): {
  api: () => ApplyParams;
  clearGen: () => number;
} {
  let applyParams: ApplyParams | undefined;
  let clearGen: { current: number } | undefined;
  function Host(): ReactElement {
    const live = useLiveSearchParams();
    // Capturing in an effect rather than during render keeps the host free of
    // render-phase side effects. Testing Library flushes effects inside act,
    // so both handles are set by the time render() returns.
    useEffect(() => {
      applyParams = live.applyParams;
      clearGen = live.clearGen;
    });
    const location = useLocation();
    return <div data-testid="search">{location.search}</div>;
  }
  const router = createMemoryRouter([{ path: "/library", element: <Host /> }], {
    initialEntries: [initialEntry],
  });
  render(<RouterProvider router={router} />);
  return {
    api: () => {
      if (applyParams === undefined) throw new Error("hook not mounted");
      return applyParams;
    },
    clearGen: () => {
      if (clearGen === undefined) throw new Error("hook not mounted");
      return clearGen.current;
    },
  };
}

describe("useLiveSearchParams", () => {
  test("a write lands in the URL", async () => {
    const host = renderHost("/library");
    host.api()((params) => {
      params.set("a", "1");
      return params;
    });
    await waitFor(() => {
      expect(screen.getByTestId("search").textContent).toContain("a=1");
    });
  });

  test("two same-tick writers both land; the second builds on the first", async () => {
    const host = renderHost("/library?keep=yes");
    const apply = host.api();
    apply((params) => {
      params.set("a", "1");
      return params;
    });
    apply((params) => {
      params.set("b", "2");
      return params;
    });
    await waitFor(() => {
      const search = screen.getByTestId("search").textContent;
      expect(search).toContain("a=1");
      expect(search).toContain("b=2");
      expect(search).toContain("keep=yes");
    });
  });

  test("a clearing write bumps the draft-invalidation generation; a plain write does not", async () => {
    const host = renderHost("/library?a=1");
    const before = host.clearGen();
    host.api()((params) => {
      params.set("b", "2");
      return params;
    });
    expect(host.clearGen()).toBe(before);
    host.api()(
      (params) => {
        params.delete("a");
        return params;
      },
      { clears: true },
    );
    expect(host.clearGen()).toBe(before + 1);
    await waitFor(() => {
      expect(screen.getByTestId("search").textContent).not.toContain("a=1");
    });
  });

  test("a mutator sees external URL changes from before the write", async () => {
    const host = renderHost("/library?x=0");
    let seen = "";
    host.api()((params) => {
      seen = params.get("x") ?? "";
      params.set("x", "1");
      return params;
    });
    expect(seen).toBe("0");
    await waitFor(() => {
      expect(screen.getByTestId("search").textContent).toContain("x=1");
    });
  });
});
