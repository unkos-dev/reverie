import { describe, expect, test } from "vite-plus/test";

import { paramsFromSearch } from "./library-params";

function search(qs: string): URLSearchParams {
  return new URLSearchParams(qs);
}

describe("paramsFromSearch", () => {
  test("returns an empty object for empty search params", () => {
    expect(paramsFromSearch(search(""))).toEqual({});
  });

  test("parses the known params", () => {
    const result = paramsFromSearch(
      search("cursor=abc&author=a1&series=s1&shelf=sh1&q=war&sort=title"),
    );
    expect(result).toEqual({
      cursor: "abc",
      author: "a1",
      series: "s1",
      shelf: "sh1",
      q: "war",
      sort: "title",
    });
  });

  test("drops unknown keys silently", () => {
    const result = paramsFromSearch(search("evil=1&sort=title"));
    expect(result).toEqual({ sort: "title" });
    expect((result as Record<string, unknown>).evil).toBeUndefined();
  });

  test("drops invalid sort values rather than passing them through", () => {
    expect(paramsFromSearch(search("sort=evil"))).toEqual({});
    expect(paramsFromSearch(search("sort="))).toEqual({});
  });

  test("accepts all valid sort enum values", () => {
    expect(paramsFromSearch(search("sort=recent")).sort).toBe("recent");
    expect(paramsFromSearch(search("sort=title")).sort).toBe("title");
    expect(paramsFromSearch(search("sort=author")).sort).toBe("author");
  });

  test("does NOT include the `sort` key when the URL omits it", () => {
    const result = paramsFromSearch(search("cursor=abc"));
    expect("sort" in result).toBe(false);
  });

  test("preserves empty-string filter values (caller decides semantics)", () => {
    // The backend treats `?q=` as "no query"; the parser is purely a
    // shape-conversion layer and forwards what's there.
    const result = paramsFromSearch(search("q="));
    expect(result.q).toBe("");
  });
});
