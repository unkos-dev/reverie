import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, requestPasswordReset } from "@/api/auth";

import { Component as AuthForgot } from "@/routes/auth-forgot-password";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("@/api/auth");

async function renderForgot(): Promise<void> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });

  const routes: RouteObject[] = [
    { path: "/forgot-password", element: <AuthForgot /> },
    { path: "/login", element: <div data-testid="login-page">Login</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/forgot-password"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(fetchSetupStatus).mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    email_recovery_enabled: true,
    password_min_length: 15,
    password_max_length: 256,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("auth-forgot-password", () => {
  test.each(["ada@example.com", "unknown@example.com"])(
    "requests recovery generically for %s",
    async (email) => {
      vi.mocked(requestPasswordReset).mockResolvedValue(undefined);
      await renderForgot();
      const user = userEvent;
      await user.type(screen.getByLabelText("Email"), email);
      await user.click(screen.getByRole("button", { name: "Send reset link" }));
      const title = screen.getByRole("heading", { name: "Check your email" });
      await expect.element(title).toHaveFocus();
      expect(requestPasswordReset).toHaveBeenCalledExactlyOnceWith(email);
      await expect
        .element(
          screen.getByText(
            `Check ${email} for a password reset link. The email says when the link expires.`,
          ),
        )
        .toBeInTheDocument();
      await expect
        .element(screen.getByRole("link", { name: "Back to sign in" }))
        .toHaveAttribute("href", "/login");
      await user.click(screen.getByRole("button", { name: "Use a different email" }));
      await expect.element(screen.getByLabelText("Email")).toHaveValue("");
      expect(requestPasswordReset).toHaveBeenCalledTimes(1);
    },
  );
  test("shows unavailable recovery without a form", async () => {
    vi.mocked(fetchSetupStatus).mockResolvedValue({
      setup_required: false,
      local_auth_enabled: true,
      oidc_enabled: false,
      email_recovery_enabled: false,
      password_min_length: 15,
      password_max_length: 256,
    });
    await renderForgot();
    await expect
      .element(
        screen.getByText(
          "Password reset by email isn't available on this server. Ask your administrator for a link to reset your password.",
        ),
      )
      .toBeInTheDocument();
    await expect.element(screen.getByRole("textbox")).not.toBeInTheDocument();
    expect(requestPasswordReset).not.toHaveBeenCalled();
  });
  test("redirects when local authentication is disabled", async () => {
    vi.mocked(fetchSetupStatus).mockResolvedValue({
      setup_required: false,
      local_auth_enabled: false,
      oidc_enabled: true,
      email_recovery_enabled: false,
      password_min_length: 15,
      password_max_length: 256,
    });
    await renderForgot();
    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
  });
  test("blocks an invalid address without a request", async () => {
    await renderForgot();
    const user = userEvent;
    await user.click(screen.getByRole("button", { name: "Send reset link" }));
    const email = screen.getByLabelText("Email");
    await expect.element(email).toHaveAccessibleDescription("Enter a valid email address.");
    await expect.element(email).toHaveFocus();
    expect(requestPasswordReset).not.toHaveBeenCalled();
  });
  test.each([
    new ApiError(429, null, "Too Many Requests", "Try again later."),
    new TypeError("network failure"),
    new ApiError(503, null, "Unavailable", "private detail"),
  ])("keeps the address after a request failure", async (error) => {
    vi.mocked(requestPasswordReset).mockRejectedValue(error);
    await renderForgot();
    const user = userEvent;
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send reset link" }));
    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent(
        error instanceof ApiError && error.status === 429
          ? "Try again later."
          : "Could not send the request. Try again.",
      );
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect.element(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
    await expect.element(screen.getByRole("button", { name: "Send reset link" })).toBeEnabled();
    expect(requestPasswordReset).toHaveBeenCalledTimes(1);
  });
  test("disables duplicate submission while pending", async () => {
    let rejectRequest: ((reason: Error) => void) | undefined;
    vi.mocked(requestPasswordReset).mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectRequest = reject;
      }),
    );
    await renderForgot();
    const user = userEvent;
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send reset link" }));
    await expect.element(screen.getByRole("button", { name: "Send reset link" })).toBeDisabled();
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Could not send the request. Try again.");
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect
      .element(screen.getByRole("button", { name: "Send reset link" }))
      .not.toHaveAttribute("data-pending");
    vi.mocked(requestPasswordReset).mockResolvedValue(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Send reset link" }));
    await expect.element(screen.getByRole("heading", { name: "Check your email" })).toHaveFocus();
    expect(requestPasswordReset).toHaveBeenCalledTimes(2);
  });
  test("retries only a failed settings query", async () => {
    vi.mocked(fetchSetupStatus).mockRejectedValueOnce(new Error("offline"));
    await renderForgot();
    const user = userEvent;
    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Could not load sign-in settings.");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await expect.element(screen.getByLabelText("Email")).toBeInTheDocument();
    expect(requestPasswordReset).not.toHaveBeenCalled();
  });
});
