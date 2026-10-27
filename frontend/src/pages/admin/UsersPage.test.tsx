import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { User } from "@/api/users";
import * as usersApi from "@/api/users";
import { toast } from "sonner";
import { fetchEmailVerification } from "@/api/auth";

import { UsersPage } from "./UsersPage";

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

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.mocked(fetchEmailVerification).mockResolvedValue({
    email: "alice@example.com",
    email_verified: false,
    mail_enabled: false,
  });
  vi.useRealTimers();
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

const ADULT_ME = { ...ADMIN_ME, role: "adult" as const };

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

function renderUsersPage(
  meData: typeof ADMIN_ME | typeof ADULT_ME | null,
  users: User[] | null = null,
): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });

  if (meData !== null) {
    client.setQueryData(queryKeys.auth.me(), meData);
  }
  if (users !== null) {
    client.setQueryData(queryKeys.users.list(), users);
  }

  const routes: RouteObject[] = [
    { path: "/admin/users", element: <UsersPage /> },
    { path: "/library", element: <div data-testid="library-page">Library</div> },
  ];
  const router = createMemoryRouter(routes, {
    initialEntries: ["/admin/users"],
  });

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

test("copy gives manual guidance when clipboard is unavailable", async () => {
  const target = makeUser();
  vi.spyOn(usersApi, "issueCredentialLink").mockResolvedValue({
    token: "test-link",
    expires_at: "2026-10-23T12:00:00Z",
  });
  renderUsersPage(ADMIN_ME, [target]);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Reset password" }));
  await user.click(screen.getByRole("button", { name: "Create link" }));
  const copy = await screen.findByRole("button", { name: "Copy link" });
  const descriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");
  Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
  try {
    await user.click(copy);
    expect(toast.error).toHaveBeenCalledWith(
      "Couldn't copy the link. Select the link and copy it manually.",
    );
    expect(screen.getByLabelText("Credential link")).toHaveValue(
      `${window.location.origin}/reset-password#test-link`,
    );
    expect(toast.success).not.toHaveBeenCalled();
  } finally {
    if (descriptor) Object.defineProperty(navigator, "clipboard", descriptor);
  }
});
