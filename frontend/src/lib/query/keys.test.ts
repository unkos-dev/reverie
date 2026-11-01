import { expect, test } from "vite-plus/test";

import { queryKeys } from "./keys";

test("books.list sorts array-valued filter params, same as authors.resolve", () => {
  const a = queryKeys.books.list({ tag_any: ["mystery", "fantasy"] });
  const b = queryKeys.books.list({ tag_any: ["fantasy", "mystery"] });
  expect(a).toEqual(b);
});
