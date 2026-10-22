import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, requestPasswordReset, resetPassword } from "@/api/auth";

import { Component as AuthForgot } from "./auth-forgot-password";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
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
    password_min_length: 15,
    password_max_length: 256,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("auth-forgot-password", () => {
  test("requests a PIN, advances to the reset phase, then resets and routes to /login", async () => {
    vi.mocked(requestPasswordReset).mockResolvedValue(undefined);
    vi.mocked(resetPassword).mockResolvedValue(undefined);
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));

    expect(requestPasswordReset).toHaveBeenCalledWith("ada@example.com");
    expect(vi.mocked(toast.info)).toHaveBeenCalled();

    await user.type(await screen.findByLabelText("Recovery PIN"), "12345678");
    await user.type(screen.getByLabelText("New password"), "new-password-passphrase!");
    await user.click(screen.getByRole("button", { name: "Reset password" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(resetPassword).toHaveBeenCalledWith(
      "ada@example.com",
      "12345678",
      "new-password-passphrase!",
    );
  });

  test("surfaces an inline error and toast when the request fails", async () => {
    vi.mocked(requestPasswordReset).mockRejectedValue(
      new ApiError(429, null, "Too Many Requests", "Try again later."),
    );
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Try again later.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("blocks a blank email submit with an associated field error and never requests recovery", async () => {
    renderForgot();
    const user = userEvent.setup();
    const email = screen.getByLabelText("Email");

    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));

    expect(email).toHaveAccessibleDescription("Enter a valid email address.");
    expect(email).toHaveFocus();
    expect(requestPasswordReset).not.toHaveBeenCalled();
    // Still on the request phase: the PIN field has not appeared.
    expect(screen.queryByLabelText("Recovery PIN")).not.toBeInTheDocument();
  });

  test("a request failure shows a general alert and keeps the email without marking it invalid", async () => {
    vi.mocked(requestPasswordReset).mockRejectedValue(
      new ApiError(429, null, "Too Many Requests", "Try again later."),
    );
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Try again later.");
    expect(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    expect(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
  });

  test("the reset phase associates PIN and password errors and sends nothing", async () => {
    vi.mocked(requestPasswordReset).mockResolvedValue(undefined);
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));
    const pin = await screen.findByLabelText("Recovery PIN");
    await user.type(pin, "  ");
    await user.type(screen.getByLabelText("New password"), "short");
    await user.click(screen.getByRole("button", { name: "Reset password" }));

    expect(pin).toHaveAccessibleDescription("Enter the recovery PIN.");
    expect(pin).toHaveFocus();
    expect(screen.getByLabelText("New password")).toHaveAccessibleDescription(
      /Use at least 15 characters\./,
    );
    expect(resetPassword).not.toHaveBeenCalled();
  });

  test("a reset failure keeps the entries and shows a general alert without invalid fields", async () => {
    vi.mocked(requestPasswordReset).mockResolvedValue(undefined);
    vi.mocked(resetPassword).mockRejectedValue(
      new ApiError(422, null, "Unprocessable", "Invalid or expired PIN."),
    );
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));
    await user.type(await screen.findByLabelText("Recovery PIN"), "12345678");
    await user.type(screen.getByLabelText("New password"), "new-password-passphrase!");
    await user.click(screen.getByRole("button", { name: "Reset password" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Invalid or expired PIN.");
    expect(screen.getByLabelText("Recovery PIN")).toHaveValue("12345678");
    expect(screen.getByLabelText("New password")).toHaveValue("new-password-passphrase!");
    expect(screen.getByLabelText("Recovery PIN")).not.toHaveAttribute("aria-invalid");
    expect(screen.getByLabelText("New password")).not.toHaveAttribute("aria-invalid");
  });

  test("a request failure does not carry its alert into the reset phase", async () => {
    vi.mocked(requestPasswordReset)
      .mockRejectedValueOnce(new ApiError(429, null, "Too Many Requests", "Try again later."))
      .mockResolvedValueOnce(undefined);
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Try again later.");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));

    expect(await screen.findByLabelText("Recovery PIN")).toBeInTheDocument();
    expect(screen.queryByText("Try again later.")).not.toBeInTheDocument();
  });

  test("repeated click and Enter while recovery is pending send one request", async () => {
    vi.mocked(requestPasswordReset).mockReturnValue(new Promise(() => {}));
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));
    await user.keyboard("{Enter}");

    expect(requestPasswordReset).toHaveBeenCalledTimes(1);
  });

  test("repeated click and Enter while a reset is pending send one request", async () => {
    vi.mocked(requestPasswordReset).mockResolvedValue(undefined);
    vi.mocked(resetPassword).mockReturnValue(new Promise(() => {}));
    renderForgot();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Send recovery PIN" }));
    await user.type(await screen.findByLabelText("Recovery PIN"), "12345678");
    await user.type(screen.getByLabelText("New password"), "new-password-passphrase!");
    await user.click(screen.getByRole("button", { name: "Reset password" }));
    await user.click(screen.getByRole("button", { name: "Reset password" }));
    await user.keyboard("{Enter}");

    expect(resetPassword).toHaveBeenCalledTimes(1);
  });
});
