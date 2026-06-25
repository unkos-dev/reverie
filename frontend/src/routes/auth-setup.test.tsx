import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { setupAdmin } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AuthSetup } from "./auth-setup";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

type Status = { setup_required: boolean; local_auth_enabled: boolean; oidc_enabled: boolean };

function renderSetup(status: Status): void {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(queryKeys.auth.setupStatus(), status);

  const routes: RouteObject[] = [
    { path: "/auth/setup", element: <AuthSetup /> },
    { path: "/auth/login", element: <div data-testid="login-page">Login</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/auth/setup"] });

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
  vi.restoreAllMocks();
});

describe("auth-setup", () => {
  test("submitting creates the admin then routes to /auth/login", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(setupAdmin).toHaveBeenCalledWith("ada@example.com", "Ada", "hunter2hunter2");
  });

  test("surfaces an inline error and toast when setup conflicts", async () => {
    vi.mocked(setupAdmin).mockRejectedValue(
      new ApiError(409, null, "Conflict", "Setup is already complete."),
    );
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Setup is already complete.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("bounces to /auth/login once an administrator exists", async () => {
    renderSetup({ setup_required: false, local_auth_enabled: true, oidc_enabled: false });
    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
  });
});
