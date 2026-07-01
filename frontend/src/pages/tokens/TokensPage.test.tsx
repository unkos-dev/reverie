import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { Token } from "@/api/tokens";
import * as tokensApi from "@/api/tokens";

import { TokensPage } from "./TokensPage";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const ADMIN_ME = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin" as const,
  is_child: false,
  theme_preference: "system",
  csrf_token: null,
};

const ADULT_ME = { ...ADMIN_ME, role: "adult" as const };

function makeToken(overrides: Partial<Token> = {}): Token {
  return {
    id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
    name: "My Kindle",
    scopes: ["read"],
    expires_at: null,
    last_used_at: null,
    created_at: "2026-05-25T00:00:00Z",
    ...overrides,
  };
}

function renderTokensPage(
  meData: typeof ADMIN_ME | typeof ADULT_ME | null,
  tokens: Token[] | null = null,
): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  if (meData !== null) {
    client.setQueryData(queryKeys.auth.me(), meData);
  }
  if (tokens !== null) {
    client.setQueryData(queryKeys.tokens.list(), tokens);
  }

  const routes: RouteObject[] = [{ path: "/tokens", element: <TokensPage /> }];
  const router = createMemoryRouter(routes, { initialEntries: ["/tokens"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  render(<Wrapper />);
  return client;
}

describe("TokensPage", () => {
  test("is reachable for a non-admin (no redirect)", async () => {
    renderTokensPage(ADULT_ME, []);
    expect(await screen.findByText("API tokens")).toBeInTheDocument();
  });

  test("shows empty state when token list is empty", async () => {
    renderTokensPage(ADULT_ME, []);
    expect(await screen.findByText(/no tokens found/i)).toBeInTheDocument();
  });

  test("renders token rows when list has tokens", async () => {
    renderTokensPage(ADULT_ME, [makeToken()]);
    expect(await screen.findByText("My Kindle")).toBeInTheDocument();
  });

  test("shows error message on fetch failure", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(queryKeys.auth.me(), ADULT_ME);
    vi.spyOn(tokensApi, "listTokens").mockRejectedValueOnce(new Error("network error"));

    const routes: RouteObject[] = [{ path: "/tokens", element: <TokensPage /> }];
    const router = createMemoryRouter(routes, { initialEntries: ["/tokens"] });

    render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    expect(await screen.findByText(/failed to load tokens/i)).toBeInTheDocument();
  });

  test("revoke button calls revokeToken with the row's id", async () => {
    const target = makeToken();
    vi.spyOn(tokensApi, "listTokens").mockResolvedValue([target]);
    const revoke = vi.spyOn(tokensApi, "revokeToken").mockResolvedValue(undefined);
    renderTokensPage(ADULT_ME, [target]);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Revoke" }));
    expect(revoke).toHaveBeenCalledWith(target.id);
  });

  test("non-admin cannot select the admin scope", async () => {
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    expect(screen.getByLabelText("read")).toBeInTheDocument();
    expect(screen.getByLabelText("write")).toBeInTheDocument();
    expect(screen.queryByLabelText("admin")).not.toBeInTheDocument();
  });

  test("admin can select the admin scope", async () => {
    renderTokensPage(ADMIN_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    expect(screen.getByLabelText("admin")).toBeInTheDocument();
  });

  test("create dialog blocks submit with no scope selected", async () => {
    const create = vi.spyOn(tokensApi, "createToken");
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByLabelText("read"));
    await user.click(screen.getByLabelText("write"));
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  test("create dialog submits name, scopes, and expiry", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader", scopes: ["read", "write"] }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(create).toHaveBeenCalledWith({
      name: "reader",
      scopes: ["read", "write"],
      expiresInDays: 90,
    });
  });

  test("reveals the credential once, including the OPDS username/password split", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn() },
      configurable: true,
    });
    vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader" }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByText("Token created")).toBeInTheDocument();
    expect(
      screen.getByDisplayValue("rvpat_11111111-1111-4111-8111-111111111111.secretvalue"),
    ).toBeInTheDocument();
    expect(
      screen.getByDisplayValue("rvpat_11111111-1111-4111-8111-111111111111"),
    ).toBeInTheDocument();
    expect(screen.getByDisplayValue("secretvalue")).toBeInTheDocument();
  });
});
