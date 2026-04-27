import { afterEach, describe, expect, test, vi } from "vitest";
import {
  THEME_COOKIE_NAME,
  readThemeCookie,
  writeThemeCookie,
} from "./cookie";

const cookieDescriptor = Object.getOwnPropertyDescriptor(
  Document.prototype,
  "cookie",
);

afterEach(() => {
  if (cookieDescriptor) {
    Object.defineProperty(document, "cookie", cookieDescriptor);
  }
  document.cookie = "";
});

describe("readThemeCookie", () => {
  test("returns null when cookie is absent", () => {
    document.cookie = "";
    expect(readThemeCookie()).toBeNull();
  });

  test("parses 'dark'", () => {
    document.cookie = `${THEME_COOKIE_NAME}=dark`;
    expect(readThemeCookie()).toBe("dark");
  });

  test("parses 'light'", () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    expect(readThemeCookie()).toBe("light");
  });

  test("parses 'system'", () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    expect(readThemeCookie()).toBe("system");
  });

  test("returns null on malformed value", () => {
    document.cookie = `${THEME_COOKIE_NAME}=javascript:alert(1)`;
    expect(readThemeCookie()).toBeNull();
  });

  test("ignores other cookies before the theme cookie", () => {
    document.cookie = "id=abc";
    document.cookie = "other=xyz";
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    expect(readThemeCookie()).toBe("light");
  });
});

describe("writeThemeCookie", () => {
  test("writes the cookie with the canonical attribute string", () => {
    const setter = vi.fn();
    Object.defineProperty(document, "cookie", {
      configurable: true,
      get: () => "",
      set: setter,
    });

    writeThemeCookie("dark");

    expect(setter).toHaveBeenCalledTimes(1);
    const written = setter.mock.calls[0]?.[0] as string;

    // Verbatim attribute parity with backend `set_theme_cookie` —
    // see backend/src/auth/theme_cookie.rs unit test for the matching
    // assertions on the Cookie struct.
    expect(written).toContain(`${THEME_COOKIE_NAME}=dark`);
    expect(written.startsWith(`${THEME_COOKIE_NAME}=dark`)).toBe(true);
    expect(written).toContain("Path=/");
    expect(written).toContain("Max-Age=31536000");
    expect(written).toContain("SameSite=Lax");
    expect(written).toContain("Secure");
    expect(written).not.toContain("HttpOnly");
  });

  test("round-trips through document.cookie", () => {
    writeThemeCookie("light");
    expect(readThemeCookie()).toBe("light");
  });
});
