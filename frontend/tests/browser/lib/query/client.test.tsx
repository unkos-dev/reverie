import { afterEach, beforeEach, expect, test, vi } from "vite-plus/test";

import {
  invokeUnauthenticatedHandler,
  queryClient,
  setUnauthenticatedHandler,
} from "@/lib/query/client";

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
});

afterEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  vi.restoreAllMocks();
});

test("setUnauthenticatedHandler resets the once-guard", () => {
  const first = vi.fn();
  const second = vi.fn();

  setUnauthenticatedHandler(first);
  invokeUnauthenticatedHandler();
  expect(first).toHaveBeenCalledTimes(1);

  setUnauthenticatedHandler(second);
  invokeUnauthenticatedHandler();
  expect(second).toHaveBeenCalledTimes(1);
});
