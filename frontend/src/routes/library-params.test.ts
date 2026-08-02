import { describe, expect, test } from "vite-plus/test";

import {
  applySortToSearchParams,
  emptyFilterState,
  filterStateToParams,
  hasActiveFilterState,
  parseFilterParams,
  paramsFromSearch,
  PURGE_ONLY_PARAM_KEYS,
  serializeFilterParams,
  TEXT_COLUMN_OPS,
  viewFromSearch,
  type FilterState,
} from "./library-params";

function search(qs: string): URLSearchParams {
  return new URLSearchParams(qs);
}

/** Serialize a state to URL params and parse it back, for round-trip checks. */
function roundTrip(state: FilterState): FilterState {
  const params = new URLSearchParams();
  serializeFilterParams(state, params);
  return parseFilterParams(params);
}

describe("paramsFromSearch", () => {
  test("returns an empty object for empty search params", () => {
    expect(paramsFromSearch(search(""))).toEqual({});
  });

  test("parses the known params (author is a repeated multi-value key)", () => {
    const result = paramsFromSearch(
      search("cursor=abc&author=a1&series=s1&shelf=sh1&q=war&sort=title"),
    );
    expect(result).toEqual({
      cursor: "abc",
      author: ["a1"],
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

  test("drops an empty `q` (a no-op filter, not sent to the server)", () => {
    expect("q" in paramsFromSearch(search("q="))).toBe(false);
  });

  test("projects typed filters onto the flat wire shape", () => {
    const result = paramsFromSearch(
      search("title_contains=dune&pages_gte=300&status_any=unread&genre_any=fantasy"),
    );
    expect(result).toEqual({
      title_contains: "dune",
      pages_gte: 300,
      status_any: ["unread"],
      genre_any: ["fantasy"],
    });
  });

  test("never sends title_empty to the API: the backend has no such predicate", () => {
    const result = paramsFromSearch(search("title_empty=true&title_contains=dune"));
    expect(result).toEqual({ title_contains: "dune" });
    expect("title_empty" in result).toBe(false);
  });

  test("a duplicate scalar key (title_contains repeated) sends only the first value", () => {
    const result = paramsFromSearch(search("title_contains=a&title_contains=b"));
    expect(result).toEqual({ title_contains: "a" });
  });
});

describe("parseFilterParams", () => {
  test("returns an all-empty state for empty search params", () => {
    expect(parseFilterParams(search(""))).toEqual(emptyFilterState());
  });

  test("parses each text operator on its own column", () => {
    const state = parseFilterParams(
      search("title_contains=du&title_eq=Dune&title_ne=Junk&subtitle_empty=true"),
    );
    expect(state.title).toEqual({ contains: "du", eq: "Dune", ne: "Junk" });
    expect(state.subtitle).toEqual({ empty: true });
  });

  test("parses numeric ranges and the is-empty toggle", () => {
    const state = parseFilterParams(search("pages_gte=100&pages_lte=500&rating_empty=false"));
    expect(state.pages).toEqual({ gte: 100, lte: 500 });
    expect(state.rating).toEqual({ empty: false });
  });

  test("parses added-date bounds", () => {
    const state = parseFilterParams(search("created_at_gte=2026-01-01&created_at_lte=2026-06-30"));
    expect(state.addedAfter).toBe("2026-01-01");
    expect(state.addedBefore).toBe("2026-06-30");
  });

  test("parses the three vocab modes and de-duplicates each set", () => {
    const state = parseFilterParams(search("genre=a&genre=a&genre_any=b&genre_none=c"));
    expect(state.genres).toEqual({ all: ["a"], any: ["b"], none: ["c"] });
  });

  test("keeps only known status tokens", () => {
    const state = parseFilterParams(
      search("status_any=unread&status_any=reading&status_any=bogus"),
    );
    expect(state.status.any).toEqual(["unread", "reading"]);
  });

  test("drops a non-integer number", () => {
    expect(parseFilterParams(search("pages_gte=abc")).pages).toEqual({});
  });

  test("drops a digit string that overflows the safe-integer range", () => {
    // A pure-digit string past 2^53 parses to a rounded/infinite Number that
    // would serialize back as garbage ("Infinity"); it must be dropped, not
    // carried, so the round-trip stays clean.
    const huge = "1".repeat(400);
    expect(parseFilterParams(search(`pages_gte=${huge}`)).pages).toEqual({});
  });

  test("drops an ill-formed date", () => {
    expect(parseFilterParams(search("created_at_gte=2026-13-99x")).addedAfter).toBeUndefined();
  });

  test("drops a non-boolean empty toggle", () => {
    expect(parseFilterParams(search("subtitle_empty=maybe")).subtitle).toEqual({});
  });

  test("clamps an over-long value set to the server cap", () => {
    const qs = Array.from({ length: 25 }, (_, i) => `author_any=a${String(i)}`).join("&");
    expect(parseFilterParams(search(qs)).authors.any).toHaveLength(20);
  });

  test("drops a hand-crafted title_empty: the backend has no such predicate (title is NOT NULL)", () => {
    expect(parseFilterParams(search("title_empty=true")).title).toEqual({});
  });

  test("a duplicate scalar key keeps only the first value", () => {
    const state = parseFilterParams(search("title_contains=a&title_contains=b"));
    expect(state.title).toEqual({ contains: "a" });
  });
});

describe("serializeFilterParams", () => {
  test("round-trips every filter family", () => {
    const state: FilterState = {
      ...emptyFilterState(),
      q: "war",
      title: { contains: "du", eq: "Dune", ne: "Junk" },
      subtitle: { contains: "one", empty: false },
      isbn13: { eq: "9780000000001", empty: true },
      pages: { gte: 100, lte: 500 },
      rating: { gte: 3, empty: false },
      addedAfter: "2026-01-01",
      addedBefore: "2026-06-30",
      status: { any: ["unread", "reading"], none: ["abandoned"] },
      authors: { all: ["a1"], any: ["a2", "a3"], none: [] },
      tags: { all: [], any: ["scifi"], none: [] },
      genres: { all: ["fantasy"], any: [], none: ["horror"] },
      moods: { all: [], any: [], none: ["bleak"] },
      series: "s1",
      shelf: "sh1",
    };
    expect(roundTrip(state)).toEqual(state);
  });

  test("delete-then-set clears a stale param on the target search object", () => {
    const params = search("title_contains=stale&pages_gte=9");
    serializeFilterParams({ ...emptyFilterState(), q: "fresh" }, params);
    expect(params.has("title_contains")).toBe(false);
    expect(params.has("pages_gte")).toBe(false);
    expect(params.get("q")).toBe("fresh");
  });

  test("leaves non-filter params (cursor, sort, view) untouched", () => {
    const params = search("cursor=abc&sort=title&view=table&title_contains=stale");
    serializeFilterParams(emptyFilterState(), params);
    expect(params.get("cursor")).toBe("abc");
    expect(params.get("sort")).toBe("title");
    expect(params.get("view")).toBe("table");
    expect(params.has("title_contains")).toBe(false);
  });

  test("omits inactive groups entirely", () => {
    const params = new URLSearchParams();
    serializeFilterParams(emptyFilterState(), params);
    expect([...params.keys()]).toEqual([]);
  });

  test("purges a hand-crafted title_empty: any filter rewrite sweeps up the dead key", () => {
    const params = search("title_empty=true");
    serializeFilterParams(parseFilterParams(params), params);
    expect(params.has("title_empty")).toBe(false);
  });
});

describe("dead text params (no wire predicate)", () => {
  /** Every dead key with a value each live sibling op would accept. */
  function deadQuery(): string {
    return PURGE_ONLY_PARAM_KEYS.map((key) => `${key}=true`).join("&");
  }

  test("covers exactly the column/op pairs the ops table omits", () => {
    expect([...PURGE_ONLY_PARAM_KEYS].sort()).toEqual([
      "isbn_13_ne",
      "subtitle_eq",
      "subtitle_ne",
      "title_empty",
    ]);
  });

  test("parse drops every dead param instead of surfacing an unremovable chip", () => {
    expect(parseFilterParams(search(deadQuery()))).toEqual(emptyFilterState());
  });

  test("the wire projection never sends a dead param", () => {
    expect(paramsFromSearch(search(deadQuery()))).toEqual({});
  });

  test("any filter rewrite sweeps dead params off the URL", () => {
    const params = search(deadQuery());
    serializeFilterParams(parseFilterParams(params), params);
    for (const key of PURGE_ONLY_PARAM_KEYS) expect(params.has(key)).toBe(false);
  });
});

describe("TEXT_COLUMN_OPS wire sync", () => {
  test("every declared op round-trips URL -> state -> wire", () => {
    // Locks the explicit projection in `assignText` to the ops table: an op
    // added to the table without a matching projection branch fails here as
    // a dead param (parsed but never sent).
    for (const [column, ops] of Object.entries(TEXT_COLUMN_OPS)) {
      for (const op of ops) {
        const key = `${column}_${op}`;
        const raw = op === "empty" ? "true" : "x";
        const sent = op === "empty" ? true : "x";
        expect(paramsFromSearch(search(`${key}=${raw}`)), key).toEqual({ [key]: sent });
      }
    }
  });
});

describe("filterStateToParams", () => {
  test("maps grouped state to the flat wire keys", () => {
    const params = filterStateToParams({
      ...emptyFilterState(),
      status: { any: ["unread"], none: [] },
      authors: { all: ["a1"], any: [], none: ["a2"] },
      pages: { gte: 500 },
    });
    expect(params).toEqual({
      status_any: ["unread"],
      author: ["a1"],
      author_none: ["a2"],
      pages_gte: 500,
    });
  });

  test("emits nothing for a pristine state", () => {
    expect(filterStateToParams(emptyFilterState())).toEqual({});
  });

  test("drops blank q/series/shelf so an empty string is not an active filter", () => {
    const blank: FilterState = { ...emptyFilterState(), q: "", series: "", shelf: "" };
    expect(filterStateToParams(blank)).toEqual({});
    expect(hasActiveFilterState(blank)).toBe(false);
  });
});

describe("hasActiveFilterState", () => {
  test("is false for a pristine state", () => {
    expect(hasActiveFilterState(emptyFilterState())).toBe(false);
  });

  test("is true when any condition is set", () => {
    expect(hasActiveFilterState({ ...emptyFilterState(), q: "x" })).toBe(true);
    expect(
      hasActiveFilterState({ ...emptyFilterState(), status: { any: ["unread"], none: [] } }),
    ).toBe(true);
  });
});

describe("applySortToSearchParams", () => {
  test("writes the serialized stack to the sort key", () => {
    const result = applySortToSearchParams(search(""), [{ field: "title", desc: false }]);
    expect(result.get("sort")).toBe("title");
  });

  test("an empty stack deletes the sort key rather than writing an empty value", () => {
    const result = applySortToSearchParams(search("sort=title"), []);
    expect(result.has("sort")).toBe(false);
  });

  test("always deletes cursor, even alongside a non-empty stack", () => {
    const result = applySortToSearchParams(search("cursor=abc"), [{ field: "author", desc: true }]);
    expect(result.has("cursor")).toBe(false);
  });

  test("leaves unrelated params untouched", () => {
    const result = applySortToSearchParams(search("view=table&q=war"), [
      { field: "pages", desc: false },
    ]);
    expect(result.get("view")).toBe("table");
    expect(result.get("q")).toBe("war");
  });
});

describe("viewFromSearch", () => {
  test("returns each valid view", () => {
    expect(viewFromSearch(search("view=grid"))).toBe("grid");
    expect(viewFromSearch(search("view=table"))).toBe("table");
  });

  test("returns null for the retired `list` view so stale URLs degrade to the default", () => {
    expect(viewFromSearch(search("view=list"))).toBeNull();
  });

  test("returns null when the param is absent", () => {
    expect(viewFromSearch(search(""))).toBeNull();
  });

  test("returns null for an out-of-range value", () => {
    expect(viewFromSearch(search("view=xyz"))).toBeNull();
  });
});
