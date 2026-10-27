import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { setupAdmin } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AuthSetup } from "@/routes/auth-setup";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

type Status = { setup_required: boolean; local_auth_enabled: boolean; oidc_enabled: boolean };

async function renderSetup(status: Status): Promise<{ client: QueryClient }> {
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
  await render(<Wrapper />);
  return { client };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("auth-setup", () => {
  test("describes the password with the policy hint without a render element warning", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    await expect
      .element(screen.getByLabelText("Password"))
      .toHaveAccessibleDescription(/Use 15 to 256 characters\./);
    expect(warn).not.toHaveBeenCalledWith(expect.stringMatching(/Unexpected DOM element/));
  });

  test("submitting creates the admin then routes to /login", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(setupAdmin).toHaveBeenCalledWith("ada@example.com", "Ada", "hunter2hunter2hunter2");
  });

  test("surfaces an inline error and toast when setup conflicts", async () => {
    vi.mocked(setupAdmin).mockRejectedValue(
      new ApiError(409, null, "Conflict", "Setup is already complete."),
    );
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await expect.element(screen.getByRole("alert")).toHaveTextContent("Setup is already complete.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("bounces to /login once an administrator exists", async () => {
    await renderSetup({ setup_required: false, local_auth_enabled: true, oidc_enabled: false });
    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
  });

  test("sets setup_required to false in cache on success", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    const { client } = await renderSetup({
      setup_required: true,
      local_auth_enabled: true,
      oidc_enabled: false,
    });
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(client.getQueryData<Status>(queryKeys.auth.setupStatus())?.setup_required).toBe(false);
  });

  test("blocks a blank display name with an associated field error and never calls setup", async () => {
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;
    const displayName = screen.getByLabelText("Display name");

    await user.type(displayName, "   ");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await expect.element(displayName).toHaveAccessibleDescription("Enter a display name.");
    await expect.element(displayName).toHaveFocus();
    await expect.element(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
    await expect.element(screen.getByLabelText("Password")).not.toHaveAttribute("aria-invalid");
    expect(setupAdmin).not.toHaveBeenCalled();
  });

  test("associates each invalid field with its own error and focuses the first", async () => {
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada");
    await user.type(screen.getByLabelText("Password"), "short");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await expect
      .element(screen.getByLabelText("Email"))
      .toHaveAccessibleDescription("Enter a valid email address.");
    await expect
      .element(screen.getByLabelText("Password"))
      .toHaveAccessibleDescription(/Use at least 15 characters\./);
    await expect.element(screen.getByLabelText("Display name")).not.toHaveAttribute("aria-invalid");
    await expect.element(screen.getByLabelText("Email")).toHaveFocus();
    expect(setupAdmin).not.toHaveBeenCalled();
  });

  test("counts password length in Unicode scalar values", async () => {
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;
    const password = screen.getByLabelText("Password");

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.fill(password, "🙂".repeat(14));
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    await expect.element(password).toHaveAccessibleDescription(/Use at least 15 characters\./);
    expect(setupAdmin).not.toHaveBeenCalled();

    await user.fill(password, "🙂".repeat(15));
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    expect(setupAdmin).toHaveBeenCalledExactlyOnceWith("ada@example.com", "Ada", "🙂".repeat(15));
  });

  test("keeps the entered values and a general alert when setup is refused", async () => {
    vi.mocked(setupAdmin).mockRejectedValue(
      new ApiError(409, null, "Conflict", "Setup is already complete."),
    );
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));

    await expect.element(screen.getByRole("alert")).toHaveTextContent("Setup is already complete.");
    await expect.element(screen.getByLabelText("Display name")).toHaveValue("Ada");
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect.element(screen.getByLabelText("Password")).toHaveValue("hunter2hunter2hunter2");
    for (const label of ["Display name", "Email", "Password"]) {
      await expect.element(screen.getByLabelText(label)).not.toHaveAttribute("aria-invalid");
    }
  });

  test("repeated click and Enter while setup is pending send one request", async () => {
    let rejectRequest: ((reason: Error) => void) | undefined;
    vi.mocked(setupAdmin).mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectRequest = reject;
      }),
    );
    await renderSetup({ setup_required: true, local_auth_enabled: true, oidc_enabled: false });
    const user = userEvent;

    await user.type(screen.getByLabelText("Display name"), "Ada");
    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Create administrator" }));
    await user.click(screen.getByRole("button", { name: "Create administrator" }), { force: true });
    await user.keyboard("{Enter}");

    expect(setupAdmin).toHaveBeenCalledTimes(1);
    await vi.waitFor(async () => {
      await expect
        .element(
          document.querySelector<HTMLElement>('[aria-live="assertive"] [role="img"]:last-child'),
        )
        .toHaveAccessibleName("Create administrator");
    });
    rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
    await expect.element(screen.getByRole("alert")).toHaveTextContent("Request refused.");
    await expect.element(screen.getByLabelText("Display name")).toHaveValue("Ada");
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect.element(screen.getByLabelText("Password")).toHaveValue("hunter2hunter2hunter2");
    await expect
      .element(screen.getByRole("button", { name: "Create administrator" }))
      .not.toHaveAttribute("data-pending");
    vi.mocked(setupAdmin).mockResolvedValue(undefined);
    await userEvent.click(screen.getByRole("button", { name: "Create administrator" }));
    await expect.element(screen.getByTestId("login-page")).toBeInTheDocument();
    expect(setupAdmin).toHaveBeenCalledTimes(2);
  });
});
