import { render } from "vitest-browser-react";
import { page, userEvent, cdp } from "vite-plus/test/browser";
import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { toast } from "sonner";
import { ThemeProvider, useTheme } from "@/lib/theme/ThemeProvider";
import { THEME_COOKIE_NAME } from "@/lib/theme/cookie";

// Mock sonner so the "no toast" guarantee on the 401 / 5xx / network
// branches is enforced explicitly, not inferred from the absence of a
// rollback. The 422-rollback test asserts toast.error WAS called; the
// quiet branches assert it WAS NOT.
vi.mock("sonner", () => ({
  toast: {
    error: vi.fn(),
    success: vi.fn(),
    message: vi.fn(),
  },
}));

function Probe(): ReactElement {
  const ctx = useTheme();
  return (
    <div>
      <span data-testid="preference">{ctx.preference}</span>
      <span data-testid="effective">{ctx.effective}</span>
      <button onClick={() => void ctx.setPreference("dark")}>set-dark</button>
      <button onClick={() => void ctx.setPreference("light")}>set-light</button>
      <button onClick={() => void ctx.setPreference("system")}>set-system</button>
    </div>
  );
}

const fetchMock = vi.fn();
const origDataset = document.documentElement.dataset.theme;

beforeEach(async () => {
  await cdp().send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-color-scheme", value: "light" }],
  });
  document.cookie = `${THEME_COOKIE_NAME}=; Path=/; Max-Age=0`;
  document.documentElement.removeAttribute("data-theme");
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
  vi.mocked(toast.error).mockClear();
});

afterEach(() => {
  vi.unstubAllGlobals();
  document.cookie = `${THEME_COOKIE_NAME}=; Path=/; Max-Age=0`;
  if (origDataset !== undefined) {
    document.documentElement.dataset.theme = origDataset;
  } else {
    document.documentElement.removeAttribute("data-theme");
  }
});

function mockMe(themePref: string, status = 200): void {
  fetchMock.mockResolvedValueOnce({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve({ theme_preference: themePref }),
  });
}

function mockMeUnauthenticated(): void {
  fetchMock.mockResolvedValueOnce({
    ok: false,
    status: 401,
    json: () => Promise.resolve({}),
  });
}

describe("ThemeProvider initial-state derivation", () => {
  test("cookie=system + dataset.theme=dark → preference=system, effective=dark", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    document.documentElement.dataset.theme = "dark";
    mockMe("system");

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await expect.element(page.getByTestId("preference")).toHaveTextContent("system");
    await expect.element(page.getByTestId("effective")).toHaveTextContent("dark");
  });

  test("cookie=light + dataset.theme=light → both light", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await expect.element(page.getByTestId("preference")).toHaveTextContent("light");
    await expect.element(page.getByTestId("effective")).toHaveTextContent("light");
  });

  test("missing cookie + dataset.theme=dark → preference=system, effective=dark", async () => {
    document.documentElement.dataset.theme = "dark";
    mockMe("system");

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await expect.element(page.getByTestId("preference")).toHaveTextContent("system");
    await expect.element(page.getByTestId("effective")).toHaveTextContent("dark");
  });

  test("logged-out (401) keeps cookie-derived preference, no PATCH", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMeUnauthenticated();

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith("/auth/me", expect.any(Object));
    });
    await expect.element(page.getByTestId("preference")).toHaveTextContent("light");
    expect(fetchMock).toHaveBeenCalledTimes(1); // /auth/me only, no PATCH
  });
});

describe("ThemeProvider reconciliation", () => {
  test("server preference differs from cookie → server wins, cookie + DOM update", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("dark");

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    });
    await expect.element(page.getByTestId("effective")).toHaveTextContent("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
  });
});

describe("ThemeProvider setPreference", () => {
  test("optimistic update + successful PATCH commits", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    fetchMock.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: () => Promise.resolve({ theme_preference: "dark" }),
    });

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("PATCH 422 → rollback to prior preference and effective", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    // PATCH rejection. A real Response: patchTheme now rides apiFetch,
    // which reads headers and re-parses the body as Problem Details.
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ title: "invalid theme_preference", status: 422 }), {
        status: 422,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("light");
    });
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=light`);
    expect(toast.error).toHaveBeenCalled();
  });

  test("PATCH 401 (anonymous) → cookie wins, no rollback, no toast", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMeUnauthenticated();
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 401,
      json: () => Promise.resolve({}),
    });

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
    expect(toast.error).not.toHaveBeenCalled();
  });

  test("PATCH 403 (expired session / missing CSRF) → cookie wins, no rollback, no toast", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 403,
      json: () => Promise.resolve({}),
    });

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
    expect(toast.error).not.toHaveBeenCalled();
  });

  test("PATCH 428 csrf-missing (persistent, null re-hydration) → cookie wins, no rollback, no toast", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    // A session that is authenticated but carries no server-side CSRF
    // token: apiFetch injects the seeded header, the middleware answers
    // 428 csrf-missing, the wrapper refreshes once, /auth/me yields no
    // token (null hydration), the replay omits the header, and the
    // middleware answers 428 again. That persistent 428 is an auth-state
    // condition, not a theme-value validation failure, so the cookie
    // must win with no rollback and no toast.
    const csrfMissing428 = (): Response =>
      new Response(
        JSON.stringify({
          type: "https://reverie.example/probs/csrf-missing",
          title: "Precondition Required",
          status: 428,
        }),
        { status: 428, headers: { "Content-Type": "application/problem+json" } },
      );
    mockMe("light"); // 1. reconcile /auth/me
    fetchMock.mockResolvedValueOnce(csrfMissing428()); // 2. PATCH → 428
    fetchMock.mockResolvedValueOnce(
      // 3. refreshCsrfToken /auth/me → token absent (null hydration)
      new Response(JSON.stringify({ csrf_token: null }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    fetchMock.mockResolvedValueOnce(csrfMissing428()); // 4. replay → 428

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(() => {
      expect(fetchMock).toHaveBeenCalledTimes(4);
    });
    // Retry path actually ran: the wrapper re-read /auth/me between attempts.
    expect(fetchMock.mock.calls[2]?.[0]).toBe("/auth/me");
    await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
    expect(toast.error).not.toHaveBeenCalled();
  });

  test("PATCH 502 (backend down) → cookie wins, no rollback, no toast", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 502,
      json: () => Promise.resolve({}),
    });

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
    expect(toast.error).not.toHaveBeenCalled();
  });

  test("PATCH network error → cookie wins, no rollback, no toast", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    fetchMock.mockRejectedValueOnce(new TypeError("Failed to fetch"));

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await userEvent.click(page.getByRole("button", { name: "set-dark" }));

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("preference")).toHaveTextContent("dark");
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
    expect(toast.error).not.toHaveBeenCalled();
  });
});

describe("ThemeProvider system-preference reactivity", () => {
  test("preference=system + matchMedia change → effective updates without page reload", async () => {
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    document.documentElement.dataset.theme = "light";
    mockMe("system");

    await render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await expect.element(page.getByTestId("effective")).toHaveTextContent("light");

    await cdp().send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-color-scheme", value: "dark" }],
    });

    await vi.waitFor(async () => {
      await expect.element(page.getByTestId("effective")).toHaveTextContent("dark");
    });
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
