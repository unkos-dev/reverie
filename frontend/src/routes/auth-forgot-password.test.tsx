import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, requestPasswordReset } from "@/api/auth";

import { Component as AuthForgot } from "./auth-forgot-password";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("@/api/auth");

function renderForgot(): void {
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
  render(<Wrapper />);
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
      renderForgot();
      const user = userEvent.setup();
      await user.type(await screen.findByLabelText("Email"), email);
      await user.click(screen.getByRole("button", { name: "Send reset link" }));
      const title = await screen.findByRole("heading", { name: "Check your email" });
      expect(title).toHaveFocus();
      expect(requestPasswordReset).toHaveBeenCalledExactlyOnceWith(email);
      expect(
        screen.getByText(
          `Check ${email} for a password reset link. The email says when the link expires.`,
        ),
      ).toBeInTheDocument();
      expect(screen.getByRole("link", { name: "Back to sign in" })).toHaveAttribute(
        "href",
        "/login",
      );
      await user.click(screen.getByRole("button", { name: "Use a different email" }));
      expect(screen.getByLabelText("Email")).toHaveValue("");
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
    renderForgot();
    expect(
      await screen.findByText(
        "Password reset by email isn't available on this server. Ask your administrator for a link to reset your password.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
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
    renderForgot();
    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
  });
  test("blocks an invalid address without a request", async () => {
    renderForgot();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Send reset link" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Enter a valid email address.");
    expect(requestPasswordReset).not.toHaveBeenCalled();
  });
  test.each([
    new ApiError(429, null, "Too Many Requests", "Try again later."),
    new TypeError("network failure"),
    new ApiError(503, null, "Unavailable", "private detail"),
  ])("keeps the address after a request failure", async (error) => {
    vi.mocked(requestPasswordReset).mockRejectedValue(error);
    renderForgot();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send reset link" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      error instanceof ApiError && error.status === 429
        ? "Try again later."
        : "Could not send the request. Try again.",
    );
    expect(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    expect(screen.getByRole("button", { name: "Send reset link" })).toBeEnabled();
    expect(requestPasswordReset).toHaveBeenCalledTimes(1);
  });
  test("disables duplicate submission while pending", async () => {
    vi.mocked(requestPasswordReset).mockReturnValue(new Promise(() => {}));
    renderForgot();
    const user = userEvent.setup();
    await user.type(await screen.findByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send reset link" }));
    expect(screen.getByRole("button", { name: "Send reset link" })).toBeDisabled();
    expect(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
  });
  test("retries only a failed settings query", async () => {
    vi.mocked(fetchSetupStatus).mockRejectedValueOnce(new Error("offline"));
    renderForgot();
    const user = userEvent.setup();
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load sign-in settings.");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByLabelText("Email")).toBeInTheDocument();
    expect(requestPasswordReset).not.toHaveBeenCalled();
  });
});
