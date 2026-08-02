import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { register } from "@/api/auth";

import { Component as AuthRegister } from "./auth-register";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

function renderRegister(): void {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const routes: RouteObject[] = [
    { path: "/register", element: <AuthRegister /> },
    { path: "/login", element: <div data-testid="login-page">Login</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/register"] });

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

describe("auth-register", () => {
  test("submitting registers then routes to /login", async () => {
    vi.mocked(register).mockResolvedValue(undefined);
    renderRegister();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(register).toHaveBeenCalledWith("ada@example.com", "Ada", "hunter2hunter2");
  });

  test("surfaces an inline error and toast when registration is disabled", async () => {
    vi.mocked(register).mockRejectedValue(
      new ApiError(404, null, "Not Found", "Registration is disabled."),
    );
    renderRegister();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Registration is disabled.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("blocks a blank display name with an inline error and never calls register", async () => {
    renderRegister();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(register).not.toHaveBeenCalled();
  });

  test("blocks a short password with an inline error and never calls register", async () => {
    renderRegister();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "short");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(register).not.toHaveBeenCalled();
  });
});
