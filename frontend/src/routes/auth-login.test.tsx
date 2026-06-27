import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { loginLocal } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AuthLogin } from "./auth-login";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

type Status = { setup_required: boolean; local_auth_enabled: boolean; oidc_enabled: boolean };

const originalLocation = window.location;

function mockLocation(): { assign: ReturnType<typeof vi.fn> } {
  const loc = { assign: vi.fn(), href: "http://localhost/" };
  Object.defineProperty(window, "location", { configurable: true, writable: true, value: loc });
  return loc;
}

function renderLogin(status: Status): void {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(queryKeys.auth.setupStatus(), status);

  const routes: RouteObject[] = [
    { path: "/login", element: <AuthLogin /> },
    { path: "/setup", element: <div data-testid="setup-page">Setup</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/login"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: originalLocation,
  });
  vi.restoreAllMocks();
});

describe("auth-login", () => {
  test("submitting credentials calls loginLocal then navigates to /library", async () => {
    vi.mocked(loginLocal).mockResolvedValue(undefined);
    const loc = mockLocation();
    renderLogin({ setup_required: false, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    await waitFor(() => {
      expect(loginLocal).toHaveBeenCalledWith("ada@example.com", "hunter2hunter2");
      expect(loc.assign).toHaveBeenCalledWith("/library");
    });
  });

  test("surfaces an inline error and toast when login fails", async () => {
    vi.mocked(loginLocal).mockRejectedValue(
      new ApiError(422, null, "Unprocessable", "Incorrect email or password."),
    );
    renderLogin({ setup_required: false, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "wrong");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Incorrect email or password.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("hides the OIDC action when oidc is disabled", () => {
    renderLogin({ setup_required: false, local_auth_enabled: true, oidc_enabled: false });
    expect(screen.queryByRole("button", { name: /OIDC/ })).not.toBeInTheDocument();
  });

  test("shows the OIDC action and initiates OIDC when enabled", async () => {
    const loc = mockLocation();
    renderLogin({ setup_required: false, local_auth_enabled: true, oidc_enabled: true });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /OIDC/ }));

    expect(loc.assign).toHaveBeenCalledWith("/auth/oidc/login");
  });

  test("bounces to /setup on a fresh instance", async () => {
    renderLogin({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    expect(await screen.findByTestId("setup-page")).toBeInTheDocument();
  });
});
