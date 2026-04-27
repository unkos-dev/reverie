import { act, render, screen, waitFor } from "@testing-library/react";
import type { ReactElement } from "react";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  test,
  vi,
} from "vitest";
import { ThemeProvider, useTheme } from "./ThemeProvider";
import { THEME_COOKIE_NAME } from "./cookie";

interface MatchMediaShim {
  set: (matches: boolean) => void;
  trigger: () => void;
}

function installMatchMedia(initial: boolean): MatchMediaShim {
  let matches = initial;
  type Listener = (event: MediaQueryListEvent) => void;
  const listeners = new Set<Listener>();
  const mql = {
    get matches(): boolean {
      return matches;
    },
    media: "(prefers-color-scheme: dark)",
    addEventListener: (_event: string, listener: Listener) => {
      listeners.add(listener);
    },
    removeEventListener: (_event: string, listener: Listener) => {
      listeners.delete(listener);
    },
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => true,
    onchange: null,
  } as unknown as MediaQueryList;
  vi.stubGlobal("matchMedia", () => mql);
  return {
    set: (next: boolean) => {
      matches = next;
    },
    trigger: () => {
      const ev = { matches } as MediaQueryListEvent;
      for (const l of listeners) l(ev);
    },
  };
}

function Probe(): ReactElement {
  const ctx = useTheme();
  return (
    <div>
      <span data-testid="preference">{ctx.preference}</span>
      <span data-testid="effective">{ctx.effective}</span>
      <button onClick={() => void ctx.setPreference("dark")}>set-dark</button>
      <button onClick={() => void ctx.setPreference("light")}>set-light</button>
      <button onClick={() => void ctx.setPreference("system")}>
        set-system
      </button>
    </div>
  );
}

const fetchMock = vi.fn();
const origDataset = document.documentElement.dataset.theme;

beforeEach(() => {
  document.cookie = "";
  document.documentElement.removeAttribute("data-theme");
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
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
    json: async () => ({ theme_preference: themePref }),
  } as Response);
}

function mockMeUnauthenticated(): void {
  fetchMock.mockResolvedValueOnce({
    ok: false,
    status: 401,
    json: async () => ({}),
  } as Response);
}

describe("ThemeProvider initial-state derivation", () => {
  test("cookie=system + dataset.theme=dark → preference=system, effective=dark", () => {
    installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    document.documentElement.dataset.theme = "dark";
    mockMe("system");

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("preference").textContent).toBe("system");
    expect(screen.getByTestId("effective").textContent).toBe("dark");
  });

  test("cookie=light + dataset.theme=light → both light", () => {
    installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("preference").textContent).toBe("light");
    expect(screen.getByTestId("effective").textContent).toBe("light");
  });

  test("missing cookie + dataset.theme=dark → preference=system, effective=dark", () => {
    installMatchMedia(false);
    document.documentElement.dataset.theme = "dark";
    mockMe("system");

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("preference").textContent).toBe("system");
    expect(screen.getByTestId("effective").textContent).toBe("dark");
  });

  test("logged-out (401) keeps cookie-derived preference, no PATCH", async () => {
    installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMeUnauthenticated();

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith(
        "/auth/me",
        expect.any(Object),
      ),
    );
    expect(screen.getByTestId("preference").textContent).toBe("light");
    expect(fetchMock).toHaveBeenCalledTimes(1); // /auth/me only, no PATCH
  });
});

describe("ThemeProvider reconciliation", () => {
  test("server preference differs from cookie → server wins, cookie + DOM update", async () => {
    installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("dark");

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await waitFor(() =>
      expect(screen.getByTestId("preference").textContent).toBe("dark"),
    );
    expect(screen.getByTestId("effective").textContent).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=dark`);
  });
});

describe("ThemeProvider setPreference", () => {
  test("optimistic update + successful PATCH commits", async () => {
    installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    fetchMock.mockResolvedValueOnce({
      ok: true,
      status: 200,
      json: async () => ({ theme_preference: "dark" }),
    } as Response);

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await act(async () => {
      screen.getByText("set-dark").click();
    });

    await waitFor(() =>
      expect(screen.getByTestId("preference").textContent).toBe("dark"),
    );
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  test("PATCH 422 → rollback to prior preference and effective", async () => {
    installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=light`;
    document.documentElement.dataset.theme = "light";
    mockMe("light");
    // PATCH rejection
    fetchMock.mockResolvedValueOnce({
      ok: false,
      status: 422,
      json: async () => ({ error: "invalid theme_preference" }),
    } as Response);

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    await act(async () => {
      screen.getByText("set-dark").click();
    });

    await waitFor(() =>
      expect(screen.getByTestId("preference").textContent).toBe("light"),
    );
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.cookie).toContain(`${THEME_COOKIE_NAME}=light`);
  });
});

describe("ThemeProvider system-preference reactivity", () => {
  test("preference=system + matchMedia change → effective updates without page reload", async () => {
    const mql = installMatchMedia(false);
    document.cookie = `${THEME_COOKIE_NAME}=system`;
    document.documentElement.dataset.theme = "light";
    mockMe("system");

    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );

    expect(screen.getByTestId("effective").textContent).toBe("light");

    await act(async () => {
      mql.set(true);
      mql.trigger();
    });

    await waitFor(() =>
      expect(screen.getByTestId("effective").textContent).toBe("dark"),
    );
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
