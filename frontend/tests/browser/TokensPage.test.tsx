import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent, cdp } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import { toast } from "sonner";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { Token } from "@/api/tokens";
import * as tokensApi from "@/api/tokens";

import { TokensPage } from "@/pages/tokens/TokensPage";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

afterEach(async () => {
  await cleanup();
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

async function renderTokensPage(
  meData: typeof ADMIN_ME | typeof ADULT_ME | null,
  tokens: Token[] | null = null,
): Promise<QueryClient> {
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

  await render(<Wrapper />);
  if (meData !== null)
    await expect.element(page.getByRole("heading", { name: "API tokens" })).toBeInTheDocument();
  return client;
}

describe("TokensPage", () => {
  test("is reachable for a non-admin (no redirect)", async () => {
    await renderTokensPage(ADULT_ME, []);
    await expect.element(page.getByText("API tokens")).toBeInTheDocument();
  });

  test("shows empty state when token list is empty", async () => {
    await renderTokensPage(ADULT_ME, []);
    await expect.element(page.getByText(/no tokens found/i)).toBeInTheDocument();
  });

  test("renders token rows when list has tokens", async () => {
    await renderTokensPage(ADULT_ME, [makeToken()]);
    await expect.element(page.getByText("My Kindle")).toBeInTheDocument();
  });

  test("shows error message on fetch failure", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(queryKeys.auth.me(), ADULT_ME);
    vi.spyOn(tokensApi, "listTokens").mockRejectedValueOnce(new Error("network error"));

    const routes: RouteObject[] = [{ path: "/tokens", element: <TokensPage /> }];
    const router = createMemoryRouter(routes, { initialEntries: ["/tokens"] });

    await render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    await expect.element(page.getByText(/failed to load tokens/i)).toBeInTheDocument();
  });

  test("revoke button calls revokeToken with the row's id", async () => {
    const target = makeToken();
    vi.spyOn(tokensApi, "listTokens").mockResolvedValue([target]);
    const revoke = vi.spyOn(tokensApi, "revokeToken").mockResolvedValue(undefined);
    await renderTokensPage(ADULT_ME, [target]);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "Revoke" }));
    expect(revoke).toHaveBeenCalledWith(target.id);
  });

  test("non-admin is not offered the admin access level", async () => {
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.click(page.getByRole("button", { name: /Access level/ }));
    await expect.element(page.getByRole("option", { name: "Read-only" })).toBeInTheDocument();
    await expect.element(page.getByRole("option", { name: "Read & write" })).toBeInTheDocument();
    await expect.element(page.getByRole("option", { name: "Admin" })).not.toBeInTheDocument();
  });

  test("admin is offered the admin access level", async () => {
    await renderTokensPage(ADMIN_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.click(page.getByRole("button", { name: /Access level/ }));
    await expect.element(page.getByRole("option", { name: "Admin" })).toBeInTheDocument();
  });

  test("create dialog blocks submit with an empty name", async () => {
    const create = vi.spyOn(tokensApi, "createToken");
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.click(page.getByRole("button", { name: "Create" }));

    await expect
      .element(page.getByLabelText("Name"))
      .toHaveAccessibleDescription("Enter a name (1-255 characters).");
    await expect.element(page.getByLabelText("Name")).toHaveFocus();
    await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  test("a whitespace-only name is blocked and an overlong one is blocked at 256 characters", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockRejectedValue(new Error("refused"));
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "New token" }));
    const name = page.getByLabelText("Name");

    await user.type(name, "   ");
    await user.click(page.getByRole("button", { name: "Create" }));
    await expect.element(name).toHaveAccessibleDescription("Enter a name (1-255 characters).");

    await name.fill("");
    await user.click(name);
    await name.fill("a".repeat(256));
    await user.click(page.getByRole("button", { name: "Create" }));
    await expect.element(name).toHaveAccessibleDescription("Enter a name (1-255 characters).");
    expect(create).not.toHaveBeenCalled();

    await name.fill("");
    await user.click(name);
    await name.fill(` ${"a".repeat(255)} `);
    await user.click(page.getByRole("button", { name: "Create" }));
    expect(create).toHaveBeenCalledExactlyOnceWith({
      name: "a".repeat(255),
      scopes: ["write"],
      expiresInDays: 90,
    });
  });

  test("the access level hint follows the native level choice", async () => {
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "New token" }));
    const form = page.getByLabelText("Name").element().closest("form");
    if (!form) throw new Error("Create-token form missing");

    expect(new FormData(form).get("level")).toBe("write");
    await expect.element(page.getByText("View and modify library data.")).toBeInTheDocument();
    await user.click(page.getByRole("button", { name: /Access level/ }));
    await user.click(page.getByRole("option", { name: "Read-only" }));
    expect(new FormData(form).get("level")).toBe("read");
    await expect
      .element(page.getByText("View library data. Cannot make changes."))
      .toBeInTheDocument();
  });

  test("repeated click and Enter while creation is pending send one request", async () => {
    let reject: (error: Error) => void = () => {};
    const create = vi.spyOn(tokensApi, "createToken").mockReturnValue(
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
    );
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: "Create" }));
    await user.click(page.getByRole("button", { name: "Create" }), { force: true });
    await user.keyboard("{Enter}");
    await user.click(page.getByLabelText("Name"));
    await user.keyboard("{Enter}");

    expect(create).toHaveBeenCalledTimes(1);
    reject(new Error("Creation refused"));
    await expect
      .element(page.getByRole("button", { name: "Create" }))
      .not.toHaveAttribute("data-pending");
  });

  test("reopening during a deferred creation restores defaults and cannot send a second request", async () => {
    let reject: (error: Error) => void = () => {};
    const create = vi.spyOn(tokensApi, "createToken").mockReturnValue(
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
    );
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: /Expires/ }));
    await user.click(page.getByRole("option", { name: "30 days" }));
    await user.click(page.getByRole("button", { name: "Create" }));
    await user.click(page.getByRole("button", { name: "Cancel" }));
    await user.click(page.getByRole("button", { name: "New token" }));

    await expect.element(page.getByLabelText("Name")).toHaveValue("");
    await expect
      .element(page.getByRole("button", { name: /Expires/ }))
      .toMatchTextContent("90 days");
    await user.type(page.getByLabelText("Name"), "second");
    await user.click(page.getByRole("button", { name: "Create" }), { force: true });
    expect(create).toHaveBeenCalledTimes(1);
    reject(new Error("Creation refused"));
    await expect
      .element(page.getByRole("button", { name: "Create" }))
      .not.toHaveAttribute("data-pending");
  });

  test("submits the default write level, name, and expiry", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader", scopes: ["write"] }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    await renderTokensPage(ADULT_ME, []);
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

  test("native expiry retains a failed choice and restores the default on reopening", async () => {
    const create = vi
      .spyOn(tokensApi, "createToken")
      .mockRejectedValue(new Error("Creation refused"));
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "New token" }));
    const form = page.getByLabelText("Name").element().closest("form");
    if (!form) throw new Error("Create-token form missing");
    expect(new FormData(form).get("expiry")).toBe("90");
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: /Expires/ }));
    await user.click(page.getByRole("option", { name: "30 days" }));
    expect(new FormData(form).get("expiry")).toBe("30");
    await user.click(page.getByRole("button", { name: "Create" }));
    await expect.element(page.getByRole("alert")).toMatchTextContent("Creation refused");
    expect(create).toHaveBeenCalledWith({ name: "reader", scopes: ["write"], expiresInDays: 30 });
    expect(new FormData(form).get("expiry")).toBe("30");
    await expect
      .element(page.getByRole("button", { name: /Expires/ }))
      .toMatchTextContent("30 days");
    await user.click(page.getByRole("button", { name: "Cancel" }));
    await user.click(page.getByRole("button", { name: "New token" }));
    const reopened = page.getByLabelText("Name").element().closest("form");
    if (!reopened) throw new Error("Reopened form missing");
    expect(new FormData(reopened).get("expiry")).toBe("90");
    await expect
      .element(page.getByRole("button", { name: /Expires/ }))
      .toMatchTextContent("90 days");
  });

  test.each([30, 60, 365])("submits the native %s-day expiry", async (days) => {
    const create = vi
      .spyOn(tokensApi, "createToken")
      .mockRejectedValue(new Error("Creation refused"));
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: /Expires/ }));
    await user.click(page.getByRole("option", { name: `${String(days)} days` }));
    await user.click(page.getByRole("button", { name: "Create" }));
    expect(create).toHaveBeenCalledWith({ name: "reader", scopes: ["write"], expiresInDays: days });
    await expect.element(page.getByRole("alert")).toBeInTheDocument();
  });

  test.each([null, "", "0", "100"])(
    "rejects an invalid submitted expiry (%s) before mutation",
    async (expiry) => {
      const create = vi.spyOn(tokensApi, "createToken");
      await renderTokensPage(ADULT_ME, []);
      const user = userEvent;
      await user.click(page.getByRole("button", { name: "New token" }));
      const form = page.getByLabelText("Name").element().closest("form");
      if (!form) throw new Error("Create-token form missing");
      const select = form.querySelector('select[name="expiry"]');
      if (!(select instanceof HTMLSelectElement)) throw new Error("Named Select missing");
      if (expiry === null) select.removeAttribute("name");
      else {
        select.add(new Option(expiry, expiry));
        select.value = expiry;
      }
      await user.type(page.getByLabelText("Name"), "reader");
      form.requestSubmit();
      await expect.element(page.getByRole("alert")).toMatchTextContent("Choose a valid expiry.");
      expect(create).not.toHaveBeenCalled();
    },
  );

  test("maps the Never expiry option to a null lifetime", async () => {
    const create = vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "forever" }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "forever");
    await user.click(page.getByRole("button", { name: /Expires/ }));
    await user.click(page.getByRole("option", { name: "Never" }));
    await user.click(page.getByRole("button", { name: "Create" }));

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
    await renderTokensPage(ADMIN_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "ops");
    await user.click(page.getByRole("button", { name: /Access level/ }));
    await user.click(page.getByRole("option", { name: "Admin" }));
    await user.click(page.getByRole("button", { name: "Create" }));

    expect(create).toHaveBeenCalledWith({
      name: "ops",
      scopes: ["admin"],
      expiresInDays: 90,
    });
  });

  test("surfaces an error and stays on the form when creation fails", async () => {
    vi.spyOn(tokensApi, "createToken").mockRejectedValueOnce(new Error("Forbidden"));
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: "Create" }));

    await expect.element(page.getByRole("alert")).toMatchTextContent("Forbidden");
    await expect.element(page.getByText("Token created")).not.toBeInTheDocument();
  });

  test("reveals the credential once, including the OPDS username/password split", async () => {
    vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader" }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: "Create" }));

    await expect.element(page.getByText("Token created")).toBeInTheDocument();

    for (const [label, value] of [
      ["Bearer token", "rvpat_11111111-1111-4111-8111-111111111111.secretvalue"],
      ["Username", "rvpat_11111111-1111-4111-8111-111111111111"],
      ["Password", "secretvalue"],
    ]) {
      const input = page.getByRole("textbox", { name: label });
      await expect.element(input).toHaveValue(value);
      await expect.element(input).toHaveAttribute("readonly");
      await user.click(page.getByText(label));
      await expect.element(input).toHaveFocus();
      await user.keyboard("changed");
      await expect.element(input).toHaveValue(value);
      await user.click(page.getByRole("button", { name: `Copy ${label}` }));
      expect(await navigator.clipboard.readText()).toBe(value);
    }
  });

  test("omits the OPDS split when the credential has no separator", async () => {
    vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader" }),
      token: "malformedcredentialnodot",
    });
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: "Create" }));

    await expect.element(page.getByText("Token created")).toBeInTheDocument();
    await expect
      .element(page.getByRole("textbox", { name: "Bearer token" }))
      .toHaveValue("malformedcredentialnodot");
    await expect.element(page.getByText("Username")).not.toBeInTheDocument();
    await expect.element(page.getByText("Password")).not.toBeInTheDocument();
  });

  test("reports a copy failure instead of a false success", async () => {
    vi.spyOn(tokensApi, "createToken").mockResolvedValue({
      ...makeToken({ name: "reader" }),
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    await renderTokensPage(ADULT_ME, []);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "New token" }));
    await user.type(page.getByLabelText("Name"), "reader");
    await user.click(page.getByRole("button", { name: "Create" }));
    await expect.element(page.getByText("Token created")).toBeInTheDocument();

    const { targetInfo } = await cdp().send("Target.getTargetInfo");
    await cdp().send("Browser.setPermission", {
      browserContextId: targetInfo.browserContextId,
      origin: window.location.origin,
      permission: { name: "clipboard-write" },
      setting: "denied",
    });
    vi.mocked(toast.success).mockClear();
    vi.mocked(toast.error).mockClear();
    await user.click(page.getByRole("button", { name: "Copy Bearer token" }));

    await vi.waitFor(() => {
      expect(vi.mocked(toast.error)).toHaveBeenCalledWith(
        "Copy failed. Select the text and copy it manually.",
      );
    });
    expect(vi.mocked(toast.success)).not.toHaveBeenCalledWith("Copied.");
  });
});

vi.mock("@/api/tokens", { spy: true });

beforeEach(async () => {
  const { targetInfo } = await cdp().send("Target.getTargetInfo");
  await cdp().send("Browser.grantPermissions", {
    browserContextId: targetInfo.browserContextId,
    origin: window.location.origin,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });
  for (const method of Object.values(tokensApi)) {
    if (vi.isMockFunction(method)) method.mockReset();
  }
  vi.clearAllMocks();
  vi.mocked(tokensApi.listTokens).mockRejectedValue(new Error("Unmocked list request"));
});

test("a late creation failure keeps the reopened token draft and feedback", async () => {
  let reject: (error: Error) => void = () => {};
  vi.mocked(tokensApi.createToken).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    }),
  );
  await renderTokensPage(ADULT_ME, []);
  await userEvent.click(page.getByRole("button", { name: "New token" }));
  await userEvent.type(page.getByLabelText("Name"), "First reader");
  await userEvent.click(page.getByRole("button", { name: "Create" }));
  await userEvent.keyboard("{Escape}");
  await userEvent.click(page.getByRole("button", { name: "New token" }));
  await userEvent.type(page.getByLabelText("Name"), "Second reader");
  reject(new Error("Creation refused"));
  await expect.element(page.getByRole("alert")).toMatchTextContent("Creation refused");
  await expect.element(page.getByLabelText("Name")).toHaveValue("Second reader");
  await expect.element(page.getByRole("button", { name: "Create" })).toBeEnabled();
  expect(tokensApi.createToken).toHaveBeenCalledTimes(1);
});
