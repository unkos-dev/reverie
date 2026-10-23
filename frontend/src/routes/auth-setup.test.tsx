import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
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

function renderSetup(status: Status): { client: QueryClient } {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(queryKeys.auth.setupStatus(), {
    email_recovery_enabled: false,
    password_min_length: 15,
    password_max_length: 256,
    ...status,
  });

  const routes: RouteObject[] = [
    { path: "/setup", element: <AuthSetup /> },
    { path: "/login", element: <div data-testid="login-page">Login</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: ["/setup"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return { client };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("auth-setup", () => {
  test("describes the password with the policy hint without a render element warning", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    expect(screen.getByLabelText("Password")).toHaveAccessibleDescription(
      /Use 15 to 256 characters\./,
    );
    expect(warn).not.toHaveBeenCalledWith(expect.stringMatching(/Unexpected DOM element/));
  });

  test("submitting creates the admin then routes to /login", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
    expect(setupAdmin).toHaveBeenCalledWith("ada@example.com", "Ada", "hunter2hunter2hunter2");
  });

  test("surfaces an inline error and toast when setup conflicts", async () => {
    vi.mocked(setupAdmin).mockRejectedValue(
      new ApiError(409, null, "Conflict", "Setup is already complete."),
    );
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Setup is already complete.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("bounces to /login once an administrator exists", async () => {
    renderSetup({ setup_required: false, local_auth_enabled: true, oidc_enabled: false });
    expect(await screen.findByTestId("login-page")).toBeInTheDocument();
  });

  test("sets setup_required to false in cache on success", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    const { client } = renderSetup({
      setup_required: true,
      local_auth_enabled: true,
      oidc_enabled: false,
    });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await screen.findByTestId("login-page");
    expect(client.getQueryData<Status>(queryKeys.auth.setupStatus())?.setup_required).toBe(false);
  });

  test("blocks a blank display name with an associated field error and never calls setup", async () => {
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();
    const displayName = screen.getByLabelText("Display name");

    await user.type(displayName, "   ");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(displayName).toHaveAccessibleDescription("Enter a display name.");
    expect(displayName).toHaveFocus();
    expect(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
    expect(screen.getByLabelText("Password")).not.toHaveAttribute("aria-invalid");
    expect(setupAdmin).not.toHaveBeenCalled();
  });

  test("associates each invalid field with its own error and focuses the first", async () => {
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada");
    await user.type(screen.getByLabelText("Password"), "short");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(screen.getByLabelText("Email")).toHaveAccessibleDescription(
      "Enter a valid email address.",
    );
    expect(screen.getByLabelText("Password")).toHaveAccessibleDescription(
      /Use at least 15 characters\./,
    );
    expect(screen.getByLabelText("Display name")).not.toHaveAttribute("aria-invalid");
    expect(screen.getByLabelText("Email")).toHaveFocus();
    expect(setupAdmin).not.toHaveBeenCalled();
  });

  test("counts password length in Unicode scalar values", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();
    const password = screen.getByLabelText("Password");

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(password);
    await user.paste("🙂".repeat(14));
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    expect(password).toHaveAccessibleDescription(/Use at least 15 characters\./);
    expect(setupAdmin).not.toHaveBeenCalled();

    await user.click(password);
    await user.paste("🙂");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    expect(setupAdmin).toHaveBeenCalledExactlyOnceWith("ada@example.com", "Ada", "🙂".repeat(15));
  });

  test("keeps the entered values and a general alert when setup is refused", async () => {
    vi.mocked(setupAdmin).mockRejectedValue(
      new ApiError(409, null, "Conflict", "Setup is already complete."),
    );
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Setup is already complete.");
    expect(screen.getByLabelText("Display name")).toHaveValue("Ada");
    expect(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    expect(screen.getByLabelText("Password")).toHaveValue("hunter2hunter2hunter2");
    for (const label of ["Display name", "Email", "Password"]) {
      expect(screen.getByLabelText(label)).not.toHaveAttribute("aria-invalid");
    }
  });

  test("repeated click and Enter while setup is pending send one request", async () => {
    vi.mocked(setupAdmin).mockReturnValue(new Promise(() => {}));
    renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    await user.keyboard("{Enter}");

    expect(setupAdmin).toHaveBeenCalledTimes(1);
    await waitFor(() => {
      expect(
        document.querySelector('[aria-live="assertive"] [role="img"]:last-child'),
      ).toHaveAccessibleName("Create administrator");
    });
  });
});
