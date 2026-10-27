import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, register } from "@/api/auth";

import { Component as AuthRegister } from "@/routes/auth-register";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

async function renderRegister(): Promise<void> {
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
  await render(<Wrapper />);
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

describe("auth-register", () => {
  test("submitting registers then routes to /login", async () => {
    vi.mocked(register).mockResolvedValue(undefined);
    await renderRegister();
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(register).toHaveBeenCalledWith("ada@example.com", "Ada", "hunter2hunter2hunter2");
  });

  test("surfaces an inline error and toast when registration is disabled", async () => {
    vi.mocked(register).mockRejectedValue(
      new ApiError(404, null, "Not Found", "Registration is disabled."),
    );
    await renderRegister();
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await expect.element(screen.getByRole("alert")).toHaveTextContent("Registration is disabled.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("blocks a blank display name with an associated field error and never calls register", async () => {
    await renderRegister();
    const user = userEvent;
    const displayName = screen.getByLabelText("Display name");

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await expect.element(displayName).toHaveAccessibleDescription("Enter a display name.");
    await expect.element(displayName).toHaveFocus();
    await expect.element(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
    expect(register).not.toHaveBeenCalled();
  });

  test("blocks a short password with an associated field error and never calls register", async () => {
    await renderRegister();
    const user = userEvent;
    const password = screen.getByLabelText("Password");

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(password, "short");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await expect.element(password).toHaveAccessibleDescription(/Use at least 15 characters\./);
    await expect.element(password).toHaveFocus();
    await expect.element(screen.getByLabelText("Display name")).not.toHaveAttribute("aria-invalid");
    expect(register).not.toHaveBeenCalled();
  });

  test("keeps the entered values and a general alert when registration is refused", async () => {
    vi.mocked(register).mockRejectedValue(
      new ApiError(404, null, "Not Found", "Registration is disabled."),
    );
    await renderRegister();
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));

    await expect.element(screen.getByRole("alert")).toHaveTextContent("Registration is disabled.");
    await expect.element(screen.getByLabelText("Display name")).toHaveValue("Ada");
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect.element(screen.getByLabelText("Password")).toHaveValue("hunter2hunter2hunter2");
    for (const label of ["Display name", "Email", "Password"]) {
      await expect.element(screen.getByLabelText(label)).not.toHaveAttribute("aria-invalid");
    }
  });

  test("repeated click and Enter while registration is pending send one request", async () => {
    let rejectRequest: ((reason: Error) => void) | undefined;
    vi.mocked(register).mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectRequest = reject;
      }),
    );
    await renderRegister();
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create account" }));
    await user.click(screen.getByRole("button", { name: "Create account" }), { force: true });
    await user.keyboard("{Enter}");

    expect(register).toHaveBeenCalledTimes(1);
    await vi.waitFor(async () => {
      await expect
        .element(
          document.querySelector<HTMLElement>('[aria-live="assertive"] [role="img"]:last-child'),
        )
        .toHaveAccessibleName("Create account");
    });
    rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
    await expect.element(screen.getByRole("alert")).toHaveTextContent("Request refused.");
    await expect.element(screen.getByLabelText("Display name")).toHaveValue("Ada");
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect.element(screen.getByLabelText("Password")).toHaveValue("hunter2hunter2hunter2");
    await expect
      .element(screen.getByRole("button", { name: "Create account" }))
      .not.toHaveAttribute("data-pending");
    vi.mocked(register).mockResolvedValue(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Create account" }));
    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(register).toHaveBeenCalledTimes(2);
  });
});
