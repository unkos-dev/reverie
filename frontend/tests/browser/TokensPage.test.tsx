import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { afterEach, beforeEach, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { Token } from "@/api/tokens";
import * as tokensApi from "@/api/tokens";

import { TokensPage } from "@/pages/tokens/TokensPage";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));
vi.mock("@/api/tokens", { spy: true });

afterEach(async () => {
  await cleanup();
  vi.restoreAllMocks();
});

beforeEach(() => {
  for (const method of Object.values(tokensApi)) {
    if (vi.isMockFunction(method)) method.mockReset();
  }
  vi.clearAllMocks();
  vi.mocked(tokensApi.listTokens).mockRejectedValue(new Error("Unmocked list request"));
});

const ADULT_ME = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "adult" as const,
  is_child: false,
  theme_preference: "system",
  csrf_token: null,
};

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

async function renderTokensPage(): Promise<void> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(queryKeys.auth.me(), ADULT_ME);
  client.setQueryData(queryKeys.tokens.list(), []);
  const routes: RouteObject[] = [{ path: "/tokens", element: <TokensPage /> }];
  const router = createMemoryRouter(routes, { initialEntries: ["/tokens"] });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  await render(<Wrapper />);
  await expect.element(page.getByRole("heading", { name: "API tokens" })).toBeInTheDocument();
}

test("submits the default write level, name, and expiry", async () => {
  const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
    ...makeToken({ name: "reader", scopes: ["write"] }),
    token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
  });
  await renderTokensPage();
  const user = userEvent;

  await user.click(page.getByRole("button", { name: "New token" }));
  await user.type(page.getByLabelText("Name"), "reader");
  await user.click(page.getByRole("button", { name: "Create" }));

  expect(create).toHaveBeenCalledWith({
    name: "reader",
    scopes: ["write"],
    expiresInDays: 90,
  });
});
