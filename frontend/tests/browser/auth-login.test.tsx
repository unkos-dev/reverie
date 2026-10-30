import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { loginLocal } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";

import { Component as AuthLogin } from "@/routes/auth-login";

vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));
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

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.restoreAllMocks();
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
});
