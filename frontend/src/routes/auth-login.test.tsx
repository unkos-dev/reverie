import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { loginLocal } from "@/api/auth";
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
  email_recovery_enabled: boolean;
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

describe("auth-login document navigation", () => {
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
        email_recovery_enabled: false,
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

  test("shows the OIDC action and initiates OIDC when enabled", async () => {
    const loc = mockLocation();
    renderLogin({
      setup_required: false,
      local_auth_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
      oidc_enabled: true,
      email_recovery_enabled: false,
    });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /OIDC/ }));

    expect(loc.assign).toHaveBeenCalledWith("/auth/oidc/login");
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

describe("outcome document navigation", () => {
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
});
