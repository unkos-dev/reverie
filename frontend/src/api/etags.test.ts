import { beforeEach, describe, expect, test } from "vite-plus/test";

import {
  __resetEtagCacheForTesting,
  etagKeyForPath,
  getRememberedEtag,
  rememberEtag,
} from "./etags";

beforeEach(() => {
  __resetEtagCacheForTesting();
});

describe("etagKeyForPath", () => {
  test("matches the reading-state path", () => {
    expect(etagKeyForPath("/api/v1/books/abc-123/reading")).toBe("reading:abc-123");
  });

  test("matches the metadata GET and PATCH path", () => {
    expect(etagKeyForPath("/api/v1/books/abc-123/metadata")).toBe("metadata:abc-123");
  });

  test("does not match unrelated paths, including the unprotected review-queue GET", () => {
    expect(etagKeyForPath("/api/v1/shelves/abc-123")).toBeNull();
    expect(etagKeyForPath("/api/v1/books/abc-123")).toBeNull();
    expect(etagKeyForPath("/api/v1/works/abc-123/metadata")).toBeNull();
    expect(etagKeyForPath("/api/v1/manifestations/abc-123/metadata")).toBeNull();
  });
});

describe("etag cache", () => {
  test("returns null for a key that was never remembered", () => {
    expect(getRememberedEtag("reading:never-seen")).toBeNull();
  });

  test("remembers and returns the last-set value for a key", () => {
    rememberEtag("reading:abc", '"one"');
    rememberEtag("reading:abc", '"two"');
    expect(getRememberedEtag("reading:abc")).toBe('"two"');
  });

  test("keys are independent", () => {
    rememberEtag("reading:abc", '"reading-tag"');
    rememberEtag("metadata:abc", '"metadata-tag"');
    expect(getRememberedEtag("reading:abc")).toBe('"reading-tag"');
    expect(getRememberedEtag("metadata:abc")).toBe('"metadata-tag"');
  });
});
