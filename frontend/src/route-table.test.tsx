import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RouterProvider, createMemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";
import { STUB_ME } from "@/__fixtures__/auth";

import { queryClient, setUnauthenticatedHandler } from "./lib/query/client";
import { routes } from "./route-table";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

const originalLocation = window.location;

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function stubApi(mutationPath: string): void {
  vi.spyOn(globalThis, "fetch").mockImplementation((input) => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    if (url.includes("/auth/me")) return Promise.resolve(json(200, STUB_ME));
    if (url.includes("/auth/setup/status")) {
      return Promise.resolve(
        json(200, {
          setup_required: false,
          local_auth_enabled: true,
          oidc_enabled: false,
          password_min_length: 15,
          password_max_length: 256,
        }),
      );
    }
    if (url.includes(mutationPath)) return Promise.resolve(new Response(null, { status: 401 }));
    return Promise.reject(new Error(`unexpected fetch: ${url}`));
  });
}

function mockLocation(): { assign: ReturnType<typeof vi.fn> } {
  const loc = { assign: vi.fn(), href: "http://localhost/", origin: "http://localhost" };
  Object.defineProperty(window, "location", { configurable: true, writable: true, value: loc });
  return loc;
}

function renderAt(path: string): void {
  const router = createMemoryRouter(routes, { initialEntries: [path] });
  render(
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
});

afterEach(() => {
  queryClient.clear();
  setUnauthenticatedHandler(() => {});
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: originalLocation,
  });
  vi.restoreAllMocks();
});

describe("route table: unauthenticated redirect coverage", () => {
  test("a 401 from the password-change mutation redirects to /login", async () => {
    stubApi("/api/v1/account/password");
    const loc = mockLocation();
    renderAt("/account/password");
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Change password" })).toBeEnabled();
    });
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });
    expect(loc.assign).toHaveBeenCalledTimes(1);
  });

  test("a 401 from a pre-auth sign-in attempt does not redirect", async () => {
    stubApi("/auth/local/login");
    const loc = mockLocation();
    renderAt("/login");
    const user = userEvent.setup();

    await user.type(await screen.findByLabelText("Email"), "alice@example.com");
    await user.type(screen.getByLabelText("Password"), "wrong-password-1");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    expect(await screen.findByText("Incorrect email or password.")).toBeInTheDocument();
    expect(loc.assign).not.toHaveBeenCalled();
  });
});
