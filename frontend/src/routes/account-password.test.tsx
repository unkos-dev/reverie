import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { changeOwnPassword } from "@/api/auth";

import { Component as AccountPassword } from "./account-password";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

function renderChange(): void {
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
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("account-password", () => {
  test("submitting changes the password then routes to /login", async () => {
    vi.mocked(changeOwnPassword).mockResolvedValue(undefined);
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Current password"), "old-password-1");
    await user.type(screen.getByLabelText("New password"), "new-password-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(changeOwnPassword).toHaveBeenCalledWith("old-password-1", "new-password-2");
  });

  test("surfaces an inline error and toast when the current password is wrong", async () => {
    vi.mocked(changeOwnPassword).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Current password is incorrect."),
    );
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Current password"), "wrong-password");
    await user.type(screen.getByLabelText("New password"), "new-password-2");
    await user.click(screen.getByRole("button", { name: "Change password" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Current password is incorrect.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("blocks a blank current password and never calls the API", async () => {
    renderChange();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("New password"), "new-password-2");
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
    await user.type(next, "new-password-2");
    await user.clear(current);
    await user.click(screen.getByRole("button", { name: "Change password" }));
    await screen.findByRole("alert");
    expect(current).toHaveAttribute("aria-invalid", "true");
    expect(next).not.toHaveAttribute("aria-invalid");
  });
});
