import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent, cdp } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { User } from "@/api/users";
import * as usersApi from "@/api/users";
import { toast } from "sonner";

import { UsersPage } from "@/pages/admin/UsersPage";

vi.mock("@/api/auth", () => ({
  fetchEmailVerification: vi
    .fn()
    .mockResolvedValue({ email: "alice@example.com", email_verified: false, mail_enabled: false }),
  fetchSetupStatus: vi.fn().mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    email_recovery_enabled: false,
    password_min_length: 15,
    password_max_length: 256,
  }),
}));

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() } }));

vi.mock("@/api/users", { spy: true });

beforeEach(async () => {
  const { targetInfo } = await cdp().send("Target.getTargetInfo");
  await cdp().send("Browser.grantPermissions", {
    browserContextId: targetInfo.browserContextId,
    origin: window.location.origin,
    permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
  });
  for (const method of Object.values(usersApi)) {
    if (vi.isMockFunction(method)) method.mockReset();
  }
  vi.clearAllMocks();
  vi.mocked(usersApi.listUsers).mockRejectedValue(new Error("Unmocked list request"));
});

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
  has_local_password: true,
  theme_preference: "system",
  csrf_token: null,
};

function makeUser(overrides: Partial<User> = {}): User {
  return {
    id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    display_name: "Bob",
    email: null,
    role: "adult",
    is_child: false,
    disabled: false,
    has_password: true,
    credential_link_expires_at: null,
    created_at: "2026-05-25T00:00:00Z",
    updated_at: "2026-05-25T00:00:00Z",
    ...overrides,
  };
}

async function renderUsersPage(meData: typeof ADMIN_ME, users: User[]): Promise<void> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  client.setQueryData(queryKeys.auth.me(), meData);
  client.setQueryData(queryKeys.users.list(), users);

  const router = createMemoryRouter([{ path: "/admin/users", element: <UsersPage /> }], {
    initialEntries: ["/admin/users"],
  });

  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }

  await render(<Wrapper />);
  await expect.element(page.getByRole("heading", { name: "Users" })).toBeInTheDocument();
}

describe("UsersPage", () => {
  test("create-user dialog returns a link without accepting a password", async () => {
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([]);
    const create = vi.spyOn(usersApi, "createUser").mockResolvedValue({
      user: makeUser({ display_name: "Carol", email: "carol@example.com", has_password: false }),
      link: { token: "test-link", expires_at: "2026-10-23T12:00:00Z" },
    });
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last());
    const dialog = page.getByRole("dialog", { name: "Create user" }).element();
    await expect.element(page.getByLabelText(/password/i)).not.toBeInTheDocument();
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last());
    expect(create).toHaveBeenCalledWith({
      email: "carol@example.com",
      display_name: "Carol",
      role: "adult",
    });
    await expect
      .element(page.getByRole("heading", { name: "Send this link to Carol" }))
      .toBeInTheDocument();
    expect(page.getByRole("dialog", { name: "Send this link to Carol" }).element()).toBe(dialog);
    await expect.element(page.getByRole("button", { name: "Copy link" })).toHaveFocus();
    await expect
      .element(page.getByLabelText("Credential link"))
      .toHaveValue(`${window.location.origin}/set-password#test-link`);
    await expect.element(page.getByText(/If it's lost, create a new link/)).toBeInTheDocument();

    await user.click(page.getByRole("button", { name: "Copy link" }));
    expect(await navigator.clipboard.readText()).toBe(
      `${window.location.origin}/set-password#test-link`,
    );
    expect(toast.success).toHaveBeenCalledWith("Link copied.");
    await user.click(page.getByRole("button", { name: "Done" }));
    await expect.element(page.getByLabelText("Credential link")).not.toBeInTheDocument();
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("button", { name: "Create user" }).last()).toHaveFocus();
    });
  });
});
