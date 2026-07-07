import { describe, expect, test } from "vite-plus/test";

import { paramsFromSearch, viewFromSearch } from "./library-params";

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

  test("drops the legacy `recent` value (absent `sort` already gets the server default)", () => {
    expect(paramsFromSearch(search("sort=recent"))).toEqual({});
  });

  test("accepts a single-level sort", () => {
    expect(paramsFromSearch(search("sort=title")).sort).toBe("title");
    expect(paramsFromSearch(search("sort=author")).sort).toBe("author");
  });

  test("accepts a two-level sort stack, preserving a descending prefix", () => {
    expect(paramsFromSearch(search("sort=author,-created_at")).sort).toBe("author,-created_at");
  });

  test("accepts a three-level sort stack with mixed directions", () => {
    expect(paramsFromSearch(search("sort=-created_at,title,pages")).sort).toBe(
      "-created_at,title,pages",
    );
  });

  test("drops unknown fields out of a stack, keeping the valid levels", () => {
    expect(paramsFromSearch(search("sort=title,bogus,author")).sort).toBe("title,author");
  });

  test("drops a duplicate field, keeping its first occurrence", () => {
    expect(paramsFromSearch(search("sort=title,-title")).sort).toBe("title");
  });

  test("caps a four-level stack at three", () => {
    expect(paramsFromSearch(search("sort=title,-author,created_at,pages")).sort).toBe(
      "title,-author,created_at",
    );
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

describe("viewFromSearch", () => {
  test("returns each valid view", () => {
    expect(viewFromSearch(search("view=grid"))).toBe("grid");
    expect(viewFromSearch(search("view=list"))).toBe("list");
    expect(viewFromSearch(search("view=table"))).toBe("table");
  });

  test("returns null when the param is absent", () => {
    expect(viewFromSearch(search(""))).toBeNull();
  });

  test("returns null for an out-of-range value", () => {
    expect(viewFromSearch(search("view=xyz"))).toBeNull();
  });
});
