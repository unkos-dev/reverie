import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, changeOwnPassword } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AccountPassword } from "./account-password";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

function renderChange(): QueryClient {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const routes: RouteObject[] = [
    { path: "/account/password", element: <AccountPassword /> },
    { path: "/login", element: <div data-testid="login-page">Login</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/account/password"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return client;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(fetchSetupStatus).mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    password_min_length: 15,
    password_max_length: 256,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("account-password", () => {
  test("retains the loaded server policy when a background refetch fails", async () => {
    vi.mocked(fetchSetupStatus)
      .mockResolvedValueOnce({
        setup_required: false,
        local_auth_enabled: true,
        oidc_enabled: false,
        password_min_length: 24,
        password_max_length: 64,
      })
      .mockRejectedValue(new Error("temporary outage"));
    const client = renderChange();
    expect(await screen.findByText(/Use 24 to 64 characters/)).toBeInTheDocument();
    await act(async () => {
      await client.invalidateQueries({ queryKey: queryKeys.auth.setupStatus() });
    });
    expect(screen.getByRole("button", { name: "Change password" })).toBeEnabled();
    expect(screen.queryByText(/Could not load the password policy/)).not.toBeInTheDocument();
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Current password"), "old");
    await user.type(screen.getByLabelText("New password"), "sixteen-letters!!");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Use at least 24 characters.");
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });
  test("uses stricter configured bounds and leaves a short current password usable", async () => {
    vi.mocked(fetchSetupStatus).mockResolvedValue({
      setup_required: false,
      local_auth_enabled: true,
      oidc_enabled: true,
      password_min_length: 24,
      password_max_length: 64,
    });
    renderChange();
    expect(await screen.findByText(/Use 24 to 64 characters/)).toBeInTheDocument();
    const user = userEvent.setup();
    await user.type(screen.getByLabelText("Current password"), "old");
    await user.type(screen.getByLabelText("New password"), "sixteen-letters!!");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Use at least 24 characters.");
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });

  test("disables submission while policy loads and reports a load failure", async () => {
    vi.mocked(fetchSetupStatus).mockRejectedValue(new Error("unavailable"));
    renderChange();
    expect(screen.getByRole("button", { name: "Change password" })).toBeDisabled();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not load the password policy",
    );
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });
  test("submitting changes the password then routes to /login", async () => {
    vi.mocked(changeOwnPassword).mockResolvedValue(undefined);
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(changeOwnPassword).toHaveBeenCalledWith("old-password-1", "new-password-two-2");
  });

  test("surfaces an inline error and toast when the current password is wrong", async () => {
    vi.mocked(changeOwnPassword).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Current password is incorrect."),
    );
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Current password"), "wrong-password");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Current password is incorrect.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("blocks a blank current password and never calls the API", async () => {
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });

  test("blocks a short new password and never calls the API", async () => {
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "short");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });

  test("scopes aria-invalid to the field that failed", async () => {
    renderChange();
    const user = userEvent.setup();
    const current = screen.getByLabelText("Current password");
    const next = screen.getByLabelText("New password");

    // A short new password marks only the new-password input, never current.
    await user.type(current, "old-password-1");
    await user.type(next, "short");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await screen.findByRole("alert");
    expect(next).toHaveAttribute("aria-invalid", "true");
    expect(current).not.toHaveAttribute("aria-invalid");

    // A blank current password marks only the current input, never new.
    await user.clear(next);
    await user.type(next, "new-password-two-2");
    await user.clear(current);
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await screen.findByRole("alert");
    expect(current).toHaveAttribute("aria-invalid", "true");
    expect(next).not.toHaveAttribute("aria-invalid");
  });
});
