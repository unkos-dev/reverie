import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import FOUC_BODY from "@/fouc/fouc.js?raw";
import { cdp } from "vite-plus/test/browser";
import { THEME_COOKIE_NAME } from "@/lib/theme/cookie";

function runFouc(): void {
  const script = document.createElement("script");
  script.textContent = FOUC_BODY;
  document.head.append(script);
  script.remove();
}

beforeEach(() => {
  document.cookie = `${THEME_COOKIE_NAME}=; Path=/; Max-Age=0`;
  document.documentElement.removeAttribute("data-theme");
});

afterEach(() => {
  document.cookie = `${THEME_COOKIE_NAME}=; Path=/; Max-Age=0`;
  document.documentElement.removeAttribute("data-theme");
});

describe("fouc.js", () => {
  test("cookie=dark → data-theme=dark regardless of system preference", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=dark`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "light" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("cookie=light → data-theme=light regardless of system preference", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("cookie=system + system prefers dark → data-theme=dark", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("cookie=system + system prefers light → data-theme=light", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "light" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("no cookie → falls through to system resolution path", async () => {
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("malformed cookie value → falls through to system resolution path", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=javascript:alert(1)`;
    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "light" }],
    });

    runFouc();

    expect(document.documentElement.dataset.theme).toBe("light");
  });

  test("native cookie denial selects light and emits the FOUC warning", async () => {
    const session = cdp();
    const warnings: unknown[] = [];
    const observe = (event: { type: string; args: { value?: unknown }[] }): void => {
      if (event.type === "warning") warnings.push(event.args[0]?.value);
    };
    session.on("Runtime.consoleAPICalled", observe);
    await session.send("Runtime.enable");
    const frame = document.createElement("iframe");
    frame.setAttribute("sandbox", "allow-scripts");
    const messages: unknown[] = [];
    const receive = (event: MessageEvent<unknown>): void => {
      if (event.source === frame.contentWindow) messages.push(event.data);
    };
    window.addEventListener("message", receive);
    frame.srcdoc = `<script>${FOUC_BODY}</script><script>parent.postMessage(document.documentElement.dataset.theme, "*")</script>`;
    try {
      document.body.append(frame);
      await vi.waitFor(() => {
        expect(messages).toEqual(["light"]);
      });
      await vi.waitFor(() => {
        expect(
          warnings.some((message) => typeof message === "string" && /FOUC/.test(message)),
        ).toBe(true);
      });
    } finally {
      frame.remove();
      window.removeEventListener("message", receive);
      session.off("Runtime.consoleAPICalled", observe);
    }
  });
});
