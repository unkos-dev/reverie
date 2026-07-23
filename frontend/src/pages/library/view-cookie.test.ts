import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import { readViewCookie, VIEW_COOKIE_NAME, writeViewCookie } from "./view-cookie";
import type { LibraryView } from "@/routes/library-params";

const VIEWS: readonly LibraryView[] = ["grid", "table"];

const cookieDescriptor = Object.getOwnPropertyDescriptor(Document.prototype, "cookie");

afterEach(() => {
  if (cookieDescriptor) {
    Object.defineProperty(document, "cookie", cookieDescriptor);
  }
  // jsdom's `document.cookie` persists across tests in the same file — clear
  // it explicitly so a write in one test can't leak into a later read.
  document.cookie = `${VIEW_COOKIE_NAME}=; Path=/; Max-Age=0`;
});

describe("VIEW_COOKIE_NAME", () => {
  test("is a non-empty, cookie-safe token", () => {
    expect(VIEW_COOKIE_NAME.length).toBeGreaterThan(0);
    expect(VIEW_COOKIE_NAME).toMatch(/^[\w-]+$/);
  });
});

describe("writeViewCookie", () => {
  test("writes the cookie with the canonical attribute string", () => {
    const setter = vi.fn();
    Object.defineProperty(document, "cookie", {
      configurable: true,
      get: () => "",
      set: setter,
    });

    writeViewCookie("table");

    expect(setter).toHaveBeenCalledTimes(1);
    const written = setter.mock.calls[0]?.[0] as string;

    // Attribute parity with the theme cookie in lib/theme/cookie.ts, the
    // shape this one mirrors. Secure is the load-bearing one: the write is
    // unobservable by design, so nothing else in the suite would notice if
    // the attribute were dropped.
    expect(written.startsWith(`${VIEW_COOKIE_NAME}=table`)).toBe(true);
    expect(written).toContain("Path=/");
    expect(written).toContain("Max-Age=31536000");
    expect(written).toContain("SameSite=Lax");
    expect(written).toContain("Secure");
  });
});

describe("writeViewCookie / readViewCookie round-trip", () => {
  for (const view of VIEWS) {
    test(`round-trips "${view}"`, () => {
      writeViewCookie(view);
      expect(readViewCookie()).toBe(view);
    });
  }
});

describe("readViewCookie edge cases", () => {
  test("returns null when the cookie is absent", () => {
    expect(readViewCookie()).toBeNull();
  });

  test("returns null when the cookie value is malformed", () => {
    document.cookie = `${VIEW_COOKIE_NAME}=not-a-view; Path=/`;
    expect(readViewCookie()).toBeNull();
  });

  test("returns null for a stored cookie holding the retired `list` view", () => {
    document.cookie = `${VIEW_COOKIE_NAME}=list; Path=/`;
    expect(readViewCookie()).toBeNull();
  });
});
