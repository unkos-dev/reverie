import { beforeEach, describe, expect, test } from "vite-plus/test";
import { cdp } from "vite-plus/test/browser";
import { z } from "zod";

import { THEME_COOKIE_NAME, readThemeCookie, writeThemeCookie } from "@/lib/theme/cookie";

beforeEach(async () => {
  await cdp().send("Network.clearBrowserCookies");
});

describe("readThemeCookie", () => {
  test("returns null when cookie is absent", () => {
    expect(readThemeCookie()).toBeNull();
  });

  test.each(["dark", "light", "system"])("parses %s", (preference) => {
    document.cookie = `${THEME_COOKIE_NAME}=${preference}; Path=/`;
    expect(readThemeCookie()).toBe(preference);
  });

  test("returns null on malformed value", () => {
    document.cookie = `${THEME_COOKIE_NAME}=javascript:alert(1); Path=/`;
    expect(readThemeCookie()).toBeNull();
  });

  test("ignores other cookies before the theme cookie", () => {
    document.cookie = "id=abc; Path=/";
    document.cookie = "other=xyz; Path=/";
    document.cookie = `${THEME_COOKIE_NAME}=light; Path=/`;
    expect(readThemeCookie()).toBe("light");
  });
});

describe("writeThemeCookie", () => {
  test("stores the canonical cookie attributes in the browser", async () => {
    const before = Date.now() / 1000;
    writeThemeCookie("dark");
    const after = Date.now() / 1000;
    const result: unknown = await cdp().send("Network.getCookies");
    const { cookies } = z
      .object({
        cookies: z.array(
          z.object({
            name: z.string(),
            value: z.string(),
            path: z.string(),
            secure: z.boolean(),
            httpOnly: z.boolean(),
            sameSite: z.string(),
            expires: z.number(),
          }),
        ),
      })
      .parse(result);
    const cookie = cookies.find((entry) => entry.name === THEME_COOKIE_NAME);
    expect(cookie).toMatchObject({
      name: THEME_COOKIE_NAME,
      value: "dark",
      path: "/",
      secure: true,
      httpOnly: false,
      sameSite: "Lax",
    });
    if (!cookie) throw new Error("Theme cookie missing");
    expect(Math.floor(cookie.expires)).toBeGreaterThanOrEqual(Math.floor(before) + 31536000);
    expect(Math.floor(cookie.expires)).toBeLessThanOrEqual(Math.floor(after) + 31536000);
  });

  test("round-trips through document.cookie", () => {
    writeThemeCookie("light");
    expect(readThemeCookie()).toBe("light");
  });
});
