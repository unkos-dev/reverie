import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, loginLocal } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AuthLogin } from "@/routes/auth-login";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

type Status = {
  setup_required: boolean;
  local_auth_enabled: boolean;
  oidc_enabled: boolean;
  email_recovery_enabled: boolean;
  password_min_length: number;
  password_max_length: number;
};

async function renderLogin(status: Status | null, initialUrl = "/login") {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (status !== null) client.setQueryData(queryKeys.auth.setupStatus(), status);

  const routes: RouteObject[] = [
    { path: "/login", element: <AuthLogin /> },
    { path: "/setup", element: <div data-testid="setup-page">Setup</div> },
  ];
  const router = createMemoryRouter(routes, { initialEntries: [initialUrl] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
  return { client, router };
}

const localStatus: Status = {
  setup_required: false,
  local_auth_enabled: true,
  password_min_length: 15,
  password_max_length: 256,
  oidc_enabled: false,
  email_recovery_enabled: false,
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("auth-login", () => {
  test("surfaces an inline error and toast when login fails", async () => {
    vi.mocked(loginLocal).mockRejectedValue(
      new ApiError(422, null, "Unprocessable", "Incorrect email or password."),
    );
    await renderLogin({
      setup_required: false,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
      email_recovery_enabled: false,
    });
    const user = userEvent;

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "wrong");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Incorrect email or password.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("native validation owns auth errors", async () => {
    await renderLogin(localStatus);
    const user = userEvent;
    const email = screen.getByLabelText("Email");
    const password = screen.getByLabelText("Password");

    await user.type(email, "not-an-email");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    expect(loginLocal).not.toHaveBeenCalled();
    await expect.element(email).toHaveAccessibleDescription("Enter a valid email address.");
    expect(email).toBeInvalid();
    await expect.element(password).toHaveAccessibleDescription("Enter your password.");
    expect(password).toBeInvalid();
    await expect.element(email).toHaveFocus();
    await expect.element(screen.getByRole("alert")).not.toBeInTheDocument();

    vi.mocked(loginLocal).mockRejectedValue(
      new ApiError(422, null, "Unprocessable", "Incorrect email or password."),
    );
    await user.clear(email);
    await user.type(email, "ada@example.com");
    await user.type(password, "wrong");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("Incorrect email or password.");
    await expect.element(email).not.toHaveAttribute("aria-invalid");
    await expect.element(password).not.toHaveAttribute("aria-invalid");
    await expect.element(email).toHaveValue("ada@example.com");
    await expect.element(password).toHaveValue("wrong");
  });

  test("a missing password blocks submission and focuses the password field", async () => {
    await renderLogin(localStatus);
    const user = userEvent;

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    expect(loginLocal).not.toHaveBeenCalled();
    await expect.element(screen.getByLabelText("Password")).toHaveFocus();
    await expect
      .element(screen.getByLabelText("Password"))
      .toHaveAccessibleDescription("Enter your password.");
    await expect.element(screen.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
  });

  test("repeated click and Enter while a sign-in is pending send one request", async () => {
    let rejectRequest: ((reason: Error) => void) | undefined;
    vi.mocked(loginLocal).mockReturnValue(
      new Promise<void>((_resolve, reject) => {
        rejectRequest = reject;
      }),
    );

    await renderLogin(localStatus);
    const user = userEvent;

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
    await user.click(screen.getByRole("button", { name: "Sign in" }));
    await user.click(screen.getByRole("button", { name: "Sign in" }), { force: true });
    await user.keyboard("{Enter}");
    await user.click(screen.getByLabelText("Password"));
    await user.keyboard("{Enter}");

    expect(loginLocal).toHaveBeenCalledTimes(1);
    rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
    await expect.element(screen.getByRole("alert")).toHaveTextContent("Request refused.");
    await expect.element(screen.getByLabelText("Email")).toHaveValue("ada@example.com");
    await expect.element(screen.getByLabelText("Password")).toHaveValue("hunter2hunter2");
    await expect
      .element(screen.getByRole("button", { name: "Sign in" }))
      .not.toHaveAttribute("data-pending");
    vi.mocked(loginLocal).mockRejectedValue(new ApiError(429, null, "Wait", "Try later."));
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await expect.element(screen.getByRole("alert")).toHaveTextContent("Try later.");
    expect(loginLocal).toHaveBeenCalledTimes(2);
  });

  test("hides the OIDC action when oidc is disabled", async () => {
    await renderLogin({
      setup_required: false,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
      email_recovery_enabled: false,
    });
    await expect.element(screen.getByRole("button", { name: /OIDC/ })).not.toBeInTheDocument();
  });

  test("bounces to /setup on a fresh instance", async () => {
    await renderLogin({
      setup_required: true,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
      email_recovery_enabled: false,
    });
    await expect.element(screen.getByTestId("setup-page")).toBeInTheDocument();
  });
});

const outcomeStatus = {
  setup_required: false,
  local_auth_enabled: true,
  password_min_length: 15,
  password_max_length: 256,
  oidc_enabled: true,
  email_recovery_enabled: false,
};

const outcomeCases = [
  [
    "denied",
    "Sign-in cancelled",
    "Sign-in was cancelled at your provider. You can try again when you're ready.",
  ],
  ["expired", "Sign-in expired", "Start a new sign-in to continue."],
  ["unavailable", "Sign-in unavailable", "Could not complete sign-in. Try again later."],
  [
    "keys",
    "Sign-in could not be verified",
    "Try again later. If this keeps happening, contact your administrator.",
  ],
  [
    "account",
    "Unable to sign in",
    "Could not sign in with this account. Contact your administrator for help.",
  ],
  ["throttled", "Too many sign-in attempts", "Wait a moment, then try again."],
];

describe("login outcomes", () => {
  test.each(outcomeCases)(
    "renders and focuses %s without starting a sign-in",
    async (outcome, title, body) => {
      const { client } = await renderLogin(outcomeStatus, `/login?outcome=${outcome}`);
      const heading = screen.getByRole("heading", { name: title });
      await expect.element(screen.getByRole("region", { name: title })).toBeInTheDocument();
      await expect.element(screen.getByText(body)).toBeInTheDocument();
      await expect.element(heading).toHaveFocus();

      expect(loginLocal).not.toHaveBeenCalled();
      const user = userEvent;
      await user.tab();
      await expect.element(screen.getByLabelText("Email")).toHaveFocus();
      client.setQueryData(queryKeys.auth.setupStatus(), { ...outcomeStatus });
      await vi.waitFor(async () => {
        await expect.element(screen.getByLabelText("Email")).toHaveFocus();
      });
    },
  );

  test.each([
    "/login",
    "/login?outcome=",
    "/login?outcome=private-text",
    "/login?outcome=denied&outcome=keys",
  ])("ignores unsupported outcome at %s", async (url) => {
    await renderLogin(outcomeStatus, url);
    await expect.element(screen.getByRole("region")).not.toBeInTheDocument();
    await expect.element(screen.getByText("private-text")).not.toBeInTheDocument();
    await expect.element(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
  });

  test("keeps the outcome and the provider action on an OIDC-only instance", async () => {
    await renderLogin({ ...outcomeStatus, local_auth_enabled: false }, "/login?outcome=keys");
    const heading = screen.getByRole("heading", { name: "Sign-in could not be verified" });
    await expect.element(heading).toHaveFocus();
    await expect.element(screen.getByLabelText("Email")).not.toBeInTheDocument();
    const user = userEvent;
    await user.tab();
    await expect.element(screen.getByRole("button", { name: "Sign in with OIDC" })).toHaveFocus();
  });

  test("keeps local sign-in when an outcome is entered on a local-only instance", async () => {
    await renderLogin({ ...outcomeStatus, oidc_enabled: false }, "/login?outcome=account");
    await expect.element(screen.getByRole("heading", { name: "Unable to sign in" })).toHaveFocus();
    await expect.element(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    await expect
      .element(screen.getByRole("button", { name: "Sign in with OIDC" }))
      .not.toBeInTheDocument();
  });

  test("setup redirect takes precedence over an outcome", async () => {
    await renderLogin({ ...outcomeStatus, setup_required: true }, "/login?outcome=denied");
    await vi.waitFor(async () => {
      await expect.element(screen.getByTestId("setup-page")).toBeInTheDocument();
    });
    await expect.element(screen.getByRole("region")).not.toBeInTheDocument();
  });
});

describe("outcome navigation and sign-in options", () => {
  test("retains outcome focus when loading options resolve", async () => {
    let complete: ((value: Status) => void) | undefined;
    const pending = new Promise<Status>((resolve) => {
      complete = resolve;
    });
    vi.mocked(fetchSetupStatus).mockReturnValue(pending);

    await renderLogin(null, "/login?outcome=expired");
    await expect.element(screen.getByText("Loading…")).toBeInTheDocument();
    await expect.element(screen.getByRole("heading", { name: "Sign-in expired" })).toHaveFocus();

    complete?.(outcomeStatus);
    await pending;

    await expect.element(screen.getByLabelText("Email")).toBeInTheDocument();
    await expect.element(screen.getByRole("heading", { name: "Sign-in expired" })).toHaveFocus();
  });

  test("shows the fixed outcome alongside an options error", async () => {
    vi.mocked(fetchSetupStatus).mockRejectedValue(new Error("unavailable"));

    await renderLogin(null, "/login?outcome=unavailable");
    await expect
      .element(screen.getByRole("alert"))
      .toHaveTextContent("We couldn't load the sign-in options. Try reloading the page.");
    await expect
      .element(screen.getByRole("heading", { name: "Sign-in unavailable" }))
      .toHaveFocus();
  });

  test("browser history restores the outcome without provider navigation", async () => {
    const { router } = await renderLogin(outcomeStatus, "/login?outcome=denied");

    await router.navigate("/login?outcome=keys");

    await expect
      .element(screen.getByRole("heading", { name: "Sign-in could not be verified" }))
      .toHaveFocus();

    await router.navigate(-1);

    await expect.element(screen.getByRole("heading", { name: "Sign-in cancelled" })).toHaveFocus();
    expect(router.state.location.search).toBe("?outcome=denied");
  });

  test("ignores provider descriptions beside a recognised outcome", async () => {
    await renderLogin(outcomeStatus, "/login?outcome=keys&error_description=private-provider-text");
    await expect.element(screen.getByText(/private-provider-text/)).not.toBeInTheDocument();
    await expect
      .element(screen.getByRole("heading", { name: "Sign-in could not be verified" }))
      .toBeInTheDocument();
  });
});
