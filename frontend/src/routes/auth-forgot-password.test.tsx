import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { requestPasswordReset, resetPassword } from "@/api/auth";

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
    await user.type(screen.getByLabelText("New password"), "newpassw0rd!");
    await user.click(screen.getByRole("button", { name: "Reset password" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(resetPassword).toHaveBeenCalledWith("ada@example.com", "12345678", "newpassw0rd!");
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
});
