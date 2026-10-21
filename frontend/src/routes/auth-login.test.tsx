import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { fetchSetupStatus, loginLocal } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AuthLogin } from "./auth-login";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/auth");

type Status = {
  setup_required: boolean;
  local_auth_enabled: boolean;
  oidc_enabled: boolean;
  password_min_length: number;
  password_max_length: number;
};

const originalLocation = window.location;

function mockLocation(): { assign: ReturnType<typeof vi.fn> } {
  const loc = { assign: vi.fn(), href: "http://localhost/" };
  Object.defineProperty(window, "location", { configurable: true, writable: true, value: loc });
  return loc;
}

function renderLogin(status: Status | null, initialUrl = "/login") {
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
  render(<Wrapper />);
  return { client, router };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: originalLocation,
  });
  vi.restoreAllMocks();
});

describe("auth-login", () => {
  test.each(["pointer", "Enter"])(
    "submitting credentials with %s calls loginLocal once then navigates to /library",
    async (activation) => {
      vi.mocked(loginLocal).mockResolvedValue(undefined);
      const loc = mockLocation();
      renderLogin({
        setup_required: false,
        local_auth_enabled: true,
        password_min_length: 15,
        password_max_length: 256,
        oidc_enabled: false,
      });
      const user = userEvent.setup();

      await user.type(screen.getByLabelText("Email"), "ada@example.com");
      await user.type(screen.getByLabelText("Password"), "hunter2hunter2");
      if (activation === "Enter") await user.keyboard("{Enter}");
      else await user.click(screen.getByRole("button", { name: "Sign in" }));

      await waitFor(() => {
        expect(loginLocal).toHaveBeenCalledExactlyOnceWith("ada@example.com", "hunter2hunter2");
        expect(loc.assign).toHaveBeenCalledWith("/library");
      });
    },
  );

  test("surfaces an inline error and toast when login fails", async () => {
    vi.mocked(loginLocal).mockRejectedValue(
      new ApiError(422, null, "Unprocessable", "Incorrect email or password."),
    );
    renderLogin({
      setup_required: false,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
    });
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("Email"), "ada@example.com");
    await user.type(screen.getByLabelText("Password"), "wrong");
    await user.click(screen.getByRole("button", { name: "Sign in" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Incorrect email or password.");
    expect(vi.mocked(toast.error)).toHaveBeenCalled();
  });

  test("hides the OIDC action when oidc is disabled", () => {
    renderLogin({
      setup_required: false,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
    });
    expect(screen.queryByRole("button", { name: /OIDC/ })).not.toBeInTheDocument();
  });

  test("shows the OIDC action and initiates OIDC when enabled", async () => {
    const loc = mockLocation();
    renderLogin({
      setup_required: false,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: true,
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /OIDC/ }));

    expect(loc.assign).toHaveBeenCalledWith("/auth/oidc/login");
  });

  test("bounces to /setup on a fresh instance", async () => {
    renderLogin({
      setup_required: true,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: false,
    });
    expect(await screen.findByTestId("setup-page")).toBeInTheDocument();
  });
});

const outcomeStatus = {
  setup_required: false,
  local_auth_enabled: true,
  password_min_length: 15,
  password_max_length: 256,
  oidc_enabled: true,
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
      const loc = mockLocation();
      const { client } = renderLogin(outcomeStatus, `/login?outcome=${outcome}`);
      const heading = screen.getByRole("heading", { name: title });
      expect(screen.getByRole("region", { name: title })).toBeInTheDocument();
      expect(screen.getByText(body)).toBeInTheDocument();
      expect(heading).toHaveFocus();
      expect(loc.assign).not.toHaveBeenCalled();
      expect(loginLocal).not.toHaveBeenCalled();
      const user = userEvent.setup();
      await user.tab();
      expect(screen.getByLabelText("Email")).toHaveFocus();
      client.setQueryData(queryKeys.auth.setupStatus(), { ...outcomeStatus });
      await waitFor(() => expect(screen.getByLabelText("Email")).toHaveFocus());
      expect(loc.assign).not.toHaveBeenCalled();
      await user.click(screen.getByRole("button", { name: "Sign in with OIDC" }));
      expect(loc.assign).toHaveBeenCalledExactlyOnceWith("/auth/oidc/login");
    },
  );

  test.each([
    "/login",
    "/login?outcome=",
    "/login?outcome=private-text",
    "/login?outcome=denied&outcome=keys",
  ])("ignores unsupported outcome at %s", (url) => {
    const loc = mockLocation();
    renderLogin(outcomeStatus, url);
    expect(screen.queryByRole("region")).not.toBeInTheDocument();
    expect(screen.queryByText("private-text")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(loc.assign).not.toHaveBeenCalled();
  });

  test("keeps the outcome and the provider action on an OIDC-only instance", async () => {
    const loc = mockLocation();
    renderLogin({ ...outcomeStatus, local_auth_enabled: false }, "/login?outcome=keys");
    const heading = screen.getByRole("heading", { name: "Sign-in could not be verified" });
    expect(heading).toHaveFocus();
    expect(screen.queryByLabelText("Email")).not.toBeInTheDocument();
    const user = userEvent.setup();
    await user.tab();
    expect(screen.getByRole("button", { name: "Sign in with OIDC" })).toHaveFocus();
    expect(loc.assign).not.toHaveBeenCalled();
  });

  test("keeps local sign-in when an outcome is entered on a local-only instance", () => {
    renderLogin({ ...outcomeStatus, oidc_enabled: false }, "/login?outcome=account");
    expect(screen.getByRole("heading", { name: "Unable to sign in" })).toHaveFocus();
    expect(screen.getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in with OIDC" })).not.toBeInTheDocument();
  });

  test("setup redirect takes precedence over an outcome", async () => {
    renderLogin({ ...outcomeStatus, setup_required: true }, "/login?outcome=denied");
    await waitFor(() => expect(screen.getByTestId("setup-page")).toBeInTheDocument());
    expect(screen.queryByRole("region")).not.toBeInTheDocument();
  });
});

describe("outcome navigation and sign-in options", () => {
  test("retains outcome focus when loading options resolve", async () => {
    let complete: ((value: Status) => void) | undefined;
    const pending = new Promise<Status>((resolve) => {
      complete = resolve;
    });
    vi.mocked(fetchSetupStatus).mockReturnValue(pending);
    const loc = mockLocation();
    renderLogin(null, "/login?outcome=expired");
    expect(screen.getByText("Loading…")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Sign-in expired" })).toHaveFocus();
    await act(async () => {
      complete?.(outcomeStatus);
      await pending;
    });
    expect(await screen.findByLabelText("Email")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Sign-in expired" })).toHaveFocus();
    expect(loc.assign).not.toHaveBeenCalled();
  });

  test("shows the fixed outcome alongside an options error", async () => {
    vi.mocked(fetchSetupStatus).mockRejectedValue(new Error("unavailable"));
    const loc = mockLocation();
    renderLogin(null, "/login?outcome=unavailable");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "We couldn't load the sign-in options. Try reloading the page.",
    );
    expect(screen.getByRole("heading", { name: "Sign-in unavailable" })).toHaveFocus();
    expect(loc.assign).not.toHaveBeenCalled();
  });

  test("browser history restores the outcome without provider navigation", async () => {
    const loc = mockLocation();
    const { router } = renderLogin(outcomeStatus, "/login?outcome=denied");
    await act(async () => {
      await router.navigate("/login?outcome=keys");
    });
    expect(screen.getByRole("heading", { name: "Sign-in could not be verified" })).toHaveFocus();
    await act(async () => {
      await router.navigate(-1);
    });
    expect(screen.getByRole("heading", { name: "Sign-in cancelled" })).toHaveFocus();
    expect(router.state.location.search).toBe("?outcome=denied");
    expect(loc.assign).not.toHaveBeenCalled();
  });

  test("ignores provider descriptions beside a recognised outcome", () => {
    renderLogin(outcomeStatus, "/login?outcome=keys&error_description=private-provider-text");
    expect(screen.queryByText(/private-provider-text/)).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "Sign-in could not be verified" }),
    ).toBeInTheDocument();
  });
});
