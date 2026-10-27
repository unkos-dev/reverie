import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, changeOwnPassword } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AccountPassword } from "@/routes/account-password";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

async function renderChange(): Promise<QueryClient> {
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
  await render(<Wrapper />);
  return client;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(fetchSetupStatus).mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    email_recovery_enabled: false,
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
        email_recovery_enabled: false,
        password_min_length: 24,
        password_max_length: 64,
      })
      .mockRejectedValue(new Error("temporary outage"));
    const client = await renderChange();
    await expect.element(screen.getByText(/Use 24 to 64 characters/)).toBeInTheDocument();

    await client.invalidateQueries({ queryKey: queryKeys.auth.setupStatus() });

    await expect.element(screen.getByRole("button", { name: "Change password" })).toBeEnabled();
    await expect
      .element(screen.getByText(/Could not load the password policy/))
      .not.toBeInTheDocument();
    const user = userEvent;
    await user.type(screen.getByLabelText("Current password"), "old");
    await user.type(screen.getByLabelText("New password"), "sixteen-letters!!");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await expect
      .element(screen.getByLabelText("New password"))
      .toHaveAccessibleDescription(/Use at least 24 characters\./);
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });
  test("uses stricter configured bounds and leaves a short current password usable", async () => {
    vi.mocked(fetchSetupStatus).mockResolvedValue({
      setup_required: false,
      local_auth_enabled: true,
      oidc_enabled: true,
      email_recovery_enabled: false,
      password_min_length: 24,
      password_max_length: 64,
    });
    await renderChange();
    await expect.element(screen.getByText(/Use 24 to 64 characters/)).toBeInTheDocument();
    const user = userEvent;
    await user.type(screen.getByLabelText("Current password"), "old");
    await user.type(screen.getByLabelText("New password"), "sixteen-letters!!");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await expect
      .element(screen.getByLabelText("New password"))
      .toHaveAccessibleDescription(/Use at least 24 characters\./);
    await expect
      .element(screen.getByLabelText("Current password"))
      .not.toHaveAttribute("aria-invalid");
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });

  test("disables submission while policy loads and reports a load failure", async () => {
    vi.mocked(fetchSetupStatus).mockRejectedValue(new Error("unavailable"));
    await renderChange();
    await expect.element(screen.getByRole("button", { name: "Change password" })).toBeDisabled();
    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Could not load the password policy. Reload to try again.");
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });
  test("submitting changes the password then routes to /login", async () => {
    vi.mocked(changeOwnPassword).mockResolvedValue(undefined);
    await renderChange();
    const user = userEvent;

    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(changeOwnPassword).toHaveBeenCalledWith("old-password-1", "new-password-two-2");
  });

  test("surfaces an inline error and toast when the current password is wrong", async () => {
    vi.mocked(changeOwnPassword).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Current password is incorrect."),
    );
    await renderChange();
    const user = userEvent;

    await user.type(screen.getByLabelText("Current password"), "wrong-password");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Current password is incorrect.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("blocks a blank current password and never calls the API", async () => {
    await renderChange();
    const user = userEvent;

    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await expect
      .element(screen.getByLabelText("Current password"))
      .toHaveAccessibleDescription("Enter your current password.");
    await expect.element(screen.getByLabelText("Current password")).toHaveFocus();
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });

  test("blocks a short new password and never calls the API", async () => {
    await renderChange();
    const user = userEvent;
    await expect.element(screen.getByText(/Use 15 to 256 characters/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "short");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await expect
      .element(screen.getByLabelText("New password"))
      .toHaveAccessibleDescription(/Use at least 15 characters\./);
    await expect.element(screen.getByLabelText("New password")).toHaveFocus();
    expect(changeOwnPassword).not.toHaveBeenCalled();
  });

  test("counts the new password in Unicode scalar values", async () => {
    vi.mocked(changeOwnPassword).mockResolvedValue(undefined);
    await renderChange();
    const user = userEvent;
    await expect.element(screen.getByText(/Use 15 to 256 characters/)).toBeInTheDocument();
    const next = screen.getByLabelText("New password");

    await user.type(screen.getByLabelText("Current password"), "old");
    await user.fill(next, "🙂".repeat(14));
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await expect.element(next).toHaveAccessibleDescription(/Use at least 15 characters\./);
    expect(changeOwnPassword).not.toHaveBeenCalled();

    await user.fill(next, "🙂".repeat(15));
    await user.click(screen.getByRole("button", { name: "Change password" }));
    expect(changeOwnPassword).toHaveBeenCalledExactlyOnceWith("old", "🙂".repeat(15));
  });

  test("keeps entered values without marking fields invalid when the server refuses", async () => {
    vi.mocked(changeOwnPassword).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Current password is incorrect."),
    );
    await renderChange();
    const user = userEvent;
    await expect.element(screen.getByText(/Use 15 to 256 characters/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("Current password"), "wrong-password");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Current password is incorrect.");
    await expect.element(screen.getByLabelText("Current password")).toHaveValue("wrong-password");
    await expect.element(screen.getByLabelText("New password")).toHaveValue("new-password-two-2");
    await expect
      .element(screen.getByLabelText("Current password"))
      .not.toHaveAttribute("aria-invalid");
    await expect.element(screen.getByLabelText("New password")).not.toHaveAttribute("aria-invalid");
  });

  test("repeated click and Enter while the change is pending send one request", async () => {
    let rejectRequest: ((reason: Error) => void) | undefined;
    vi.mocked(changeOwnPassword).mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectRequest = reject;
      }),
    );
    await renderChange();
    const user = userEvent;
    await expect.element(screen.getByText(/Use 15 to 256 characters/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-two-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await user.click(screen.getByRole("button", { name: "Change password" }), { force: true });
    await user.keyboard("{Enter}");

    expect(changeOwnPassword).toHaveBeenCalledTimes(1);
    rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
    await expect.element(screen.getByRole("alert")).toHaveTextContent("Request refused.");
    await expect.element(screen.getByLabelText("Current password")).toHaveValue("old-password-1");
    await expect.element(screen.getByLabelText("New password")).toHaveValue("new-password-two-2");
    await expect
      .element(screen.getByRole("button", { name: "Change password" }))
      .not.toHaveAttribute("data-pending");
    vi.mocked(changeOwnPassword).mockResolvedValue(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Change password" }));
    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(changeOwnPassword).toHaveBeenCalledTimes(2);
  });

  test("scopes aria-invalid to the field that failed", async () => {
    await renderChange();
    const user = userEvent;
    const current = screen.getByLabelText("Current password");
    const next = screen.getByLabelText("New password");

    await user.type(current, "old-password-1");
    await user.type(next, "short");
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await expect.element(next).toHaveAttribute("aria-invalid", "true");
    await expect.element(current).not.toHaveAttribute("aria-invalid");

    await user.clear(next);
    await user.type(next, "new-password-two-2");
    await user.clear(current);
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await expect.element(current).toHaveAttribute("aria-invalid", "true");
    await expect.element(next).not.toHaveAttribute("aria-invalid");
  });
});
