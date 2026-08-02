import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { Token } from "@/api/tokens";
import * as tokensApi from "@/api/tokens";

import { TokensPage } from "./TokensPage";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

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
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
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

  test("non-admin is not offered the admin access level", async () => {
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.click(screen.getByLabelText("Access level"));
    expect(await screen.findByRole("option", { name: "Read-only" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Read & write" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "Admin" })).not.toBeInTheDocument();
  });

  test("admin is offered the admin access level", async () => {
    renderTokensPage(ADMIN_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.click(screen.getByLabelText("Access level"));
    expect(await screen.findByRole("option", { name: "Admin" })).toBeInTheDocument();
  });

  test("create dialog blocks submit with an empty name", async () => {
    const create = vi.spyOn(tokensApi, "createToken");
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  test("submits the default write level, name, and expiry", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader", scopes: ["write"] }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(create).toHaveBeenCalledWith({
      name: "reader",
      scopes: ["write"],
      expiresInDays: 90,
    });
  });

  test("maps the Never expiry option to a null lifetime", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "forever" }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "forever");
    await user.click(screen.getByLabelText("Expires"));
    await user.click(await screen.findByRole("option", { name: "Never" }));
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(create).toHaveBeenCalledWith({
      name: "forever",
      scopes: ["write"],
      expiresInDays: null,
    });
  });

  test("admin mints an admin-level token as a single scope", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "ops", scopes: ["admin"] }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    renderTokensPage(ADMIN_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "ops");
    await user.click(screen.getByLabelText("Access level"));
    await user.click(await screen.findByRole("option", { name: "Admin" }));
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(create).toHaveBeenCalledWith({
      name: "ops",
      scopes: ["admin"],
      expiresInDays: 90,
    });
  });

  test("surfaces an error and stays on the form when creation fails", async () => {
    vi.spyOn(tokensApi, "createToken").mockRejectedValueOnce(new Error("Forbidden"));
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Forbidden");
    expect(screen.queryByText("Token created")).not.toBeInTheDocument();
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

  test("omits the OPDS split when the credential has no separator", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn() },
      configurable: true,
    });
    vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader" }),
      token: "malformedcredentialnodot",
    });
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByText("Token created")).toBeInTheDocument();
    expect(screen.getByDisplayValue("malformedcredentialnodot")).toBeInTheDocument();
    expect(screen.queryByText("Username")).not.toBeInTheDocument();
    expect(screen.queryByText("Password")).not.toBeInTheDocument();
  });

  test("reports a copy failure instead of a false success", async () => {
    vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader" }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    renderTokensPage(ADULT_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "New token" }));
    await user.type(screen.getByLabelText("Name"), "reader");
    await user.click(screen.getByRole("button", { name: "Create" }));
    await screen.findByText("Token created");

    // Install the rejecting clipboard AFTER userEvent.setup(), which otherwise
    // replaces navigator.clipboard with its own resolving stub.
    const writeText = vi.fn<(v: string) => Promise<void>>(() =>
      Promise.reject(new Error("denied")),
    );
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    // Isolate this copy from any toast calls the mint flow made earlier: the
    // sonner mock functions accumulate across the test's own actions.
    vi.mocked(toast.success).mockClear();
    vi.mocked(toast.error).mockClear();
    await user.click(screen.getByRole("button", { name: "Copy Bearer token" }));

    await vi.waitFor(() => {
      expect(vi.mocked(toast.error)).toHaveBeenCalledWith(
        "Copy failed. Select the text and copy it manually.",
      );
    });
    expect(writeText).toHaveBeenCalledWith(
      "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    );
    expect(vi.mocked(toast.success)).not.toHaveBeenCalledWith("Copied.");
  });
});
