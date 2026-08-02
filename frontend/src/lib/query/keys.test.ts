import { describe, expect, test } from "vite-plus/test";

import { queryKeys } from "./keys";

describe("queryKeys", () => {
  test("books.all is the root namespace tuple", () => {
    expect(queryKeys.books.all).toEqual(["books"]);
  });

  test("books.list keys equal-params produce structurally-equal tuples", () => {
    const a = queryKeys.books.list({ sort: "title" });
    const b = queryKeys.books.list({ sort: "title" });
    expect(a).toEqual(b);
  });

  test("books.list keys distinct-params produce distinct tuples", () => {
    const a = queryKeys.books.list({ sort: "title" });
    const b = queryKeys.books.list({ sort: "author" });
    expect(a).not.toEqual(b);
  });

  test("books.detail keys are stable for the same id", () => {
    expect(queryKeys.books.detail("abc-123")).toEqual(["books", "detail", "abc-123"]);
  });

  test("works.detail keys are distinct from books.detail with the same id", () => {
    const book = queryKeys.books.detail("abc-123");
    const work = queryKeys.works.detail("abc-123");
    expect(book[0]).toBe("books");
    expect(work[0]).toBe("works");
    expect(book).not.toEqual(work);
  });

  test("list and detail tuples start with the root namespace", () => {
    expect(queryKeys.books.list({})[0]).toBe("books");
    expect(queryKeys.books.detail("x")[0]).toBe("books");
  });

  test("books.list sorts array-valued filter params, same as authors.resolve", () => {
    const a = queryKeys.books.list({ tag_any: ["mystery", "fantasy"] });
    const b = queryKeys.books.list({ tag_any: ["fantasy", "mystery"] });
    expect(a).toEqual(b);
  });

  test("books.list keys distinct array-valued filters as distinct tuples", () => {
    const a = queryKeys.books.list({ author_none: ["a", "b"] });
    const b = queryKeys.books.list({ author_none: ["a", "c"] });
    expect(a).not.toEqual(b);
  });

  test("books.list sorting does not mutate the caller's array", () => {
    const tags = ["mystery", "fantasy"];
    queryKeys.books.list({ tag_any: tags });
    expect(tags).toEqual(["mystery", "fantasy"]);
  });
});
