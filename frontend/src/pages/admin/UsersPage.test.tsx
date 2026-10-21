import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { Role, User } from "@/api/users";
import * as usersApi from "@/api/users";
import { toast } from "sonner";
import { ApiError } from "@/api";
import { fetchSetupStatus } from "@/api/auth";

import { UsersPage } from "./UsersPage";

vi.mock("@/api/auth", () => ({
  fetchSetupStatus: vi.fn().mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    password_min_length: 15,
    password_max_length: 256,
  }),
}));

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

describe("UsersPage", () => {
  test("redirects non-admin to /library", async () => {
    renderUsersPage(ADULT_ME, []);
    expect(await screen.findByTestId("library-page")).toBeInTheDocument();
  });

  test("shows empty state when users list is empty", async () => {
    renderUsersPage(ADMIN_ME, []);
    expect(await screen.findByText(/no users found/i)).toBeInTheDocument();
  });

  test("renders user rows when list has users", async () => {
    renderUsersPage(ADMIN_ME, [makeUser()]);
    expect(await screen.findByText("Bob")).toBeInTheDocument();
  });

  test("marks self with 'you' badge", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id, display_name: "Alice" })]);
    expect(await screen.findByText("you")).toBeInTheDocument();
  });

  test("shows error message on fetch failure", async () => {
    // Pre-seed error state by leaving users cache empty and having no fetch mock.
    // Instead seed the cache with the error directly via query state manipulation.
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    client.setQueryData(queryKeys.auth.me(), ADMIN_ME);
    // Don't seed users — query will attempt fetch; mock it to fail.
    vi.spyOn(usersApi, "listUsers").mockRejectedValueOnce(new Error("network error"));

    const routes: RouteObject[] = [{ path: "/admin/users", element: <UsersPage /> }];
    const router = createMemoryRouter(routes, { initialEntries: ["/admin/users"] });

    render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    expect(await screen.findByText(/failed to load users/i)).toBeInTheDocument();
  });

  test("shows table with correct columns when users present", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ email: "bob@example.com" })]);
    expect(await screen.findByText("Bob")).toBeInTheDocument();
    expect(screen.getByText("bob@example.com")).toBeInTheDocument();
    expect(screen.getAllByRole("columnheader").map((header) => header.textContent)).toEqual([
      "Name",
      "Email",
      "Role",
      "Status",
      "Actions",
    ]);
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });

  test.each<{ from: Role; to: Role }>([
    { from: "adult", to: "child" },
    { from: "admin", to: "child" },
    { from: "child", to: "adult" },
    { from: "child", to: "admin" },
  ])("changes $from to $to through the Role dropdown", async ({ from, to }) => {
    const target = makeUser({ role: from, is_child: from === "child" });
    const updated = { ...target, role: to, is_child: to === "child" };
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const changeRole = vi.spyOn(usersApi, "updateUserRole").mockResolvedValue(updated);
    const childStatus = vi.spyOn(usersApi, "updateUserChildStatus");
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response(JSON.stringify(ADMIN_ME)));
    const client = renderUsersPage(ADMIN_ME, [target]);
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: /Role/ }));
    vi.mocked(usersApi.listUsers).mockResolvedValue([updated]);
    await user.click(await screen.findByRole("option", { name: to }));

    await waitFor(() => {
      expect(changeRole).toHaveBeenCalledWith(target.id, to);
    });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /Role/ })).toHaveTextContent(to);
    });
    expect(childStatus).not.toHaveBeenCalled();
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.users.all });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.auth.me() });
  });

  test("keeps the admin role and reports a last-admin refusal", async () => {
    const target = makeUser({ role: "admin" });
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    vi.spyOn(usersApi, "updateUserRole").mockRejectedValue(
      new ApiError(422, null, "Validation failed", "would leave zero admins"),
    );
    renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: /Role/ }));
    await user.click(await screen.findByRole("option", { name: "child" }));

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Role update failed: would leave zero admins");
    });
    expect(screen.getByRole("button", { name: /Role/ })).toHaveTextContent("admin");
  });

  test("renders a disabled badge for a disabled account", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ disabled: true })]);
    expect(await screen.findByText("disabled")).toBeInTheDocument();
  });

  test("renders an active badge for an enabled account", async () => {
    renderUsersPage(ADMIN_ME, [makeUser()]);
    expect(await screen.findByText("active")).toBeInTheDocument();
  });

  test("disable button calls setAccountStatus with disabled=true", async () => {
    const target = makeUser();
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const setStatus = vi
      .spyOn(usersApi, "setAccountStatus")
      .mockResolvedValue({ ...target, disabled: true });
    renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Disable" }));
    expect(setStatus).toHaveBeenCalledWith(target.id, true);
  });

  test("shows an Enable control for a disabled account", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ disabled: true })]);
    expect(await screen.findByRole("button", { name: "Enable" })).toBeInTheDocument();
  });

  test("enable button calls setAccountStatus with disabled=false", async () => {
    const target = makeUser({ disabled: true });
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const setStatus = vi
      .spyOn(usersApi, "setAccountStatus")
      .mockResolvedValue({ ...target, disabled: false });
    renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Enable" }));
    expect(setStatus).toHaveBeenCalledWith(target.id, false);
  });

  test("offers no disable control for the calling admin's own row", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id, display_name: "Alice" })]);
    await screen.findByText("Alice");
    expect(screen.queryByRole("button", { name: "Disable" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Enable" })).not.toBeInTheDocument();
  });

  test("create-user dialog returns a link without accepting a password", async () => {
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([]);
    const create = vi.spyOn(usersApi, "createUser").mockResolvedValue({
      user: makeUser({ display_name: "Carol", email: "carol@example.com", has_password: false }),
      link: { token: "test-link", expires_at: "2026-10-23T12:00:00Z" },
    });
    renderUsersPage(ADMIN_ME, []);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Create user" }));
    expect(screen.queryByLabelText(/password/i)).not.toBeInTheDocument();
    await user.type(screen.getByLabelText("Display name"), "Carol");
    await user.type(screen.getByLabelText("Email"), "carol@example.com");
    await user.click(screen.getByRole("button", { name: "Create user" }));
    expect(create).toHaveBeenCalledWith({
      email: "carol@example.com",
      display_name: "Carol",
      role: "adult",
    });
    expect(
      await screen.findByRole("heading", { name: "Send this link to Carol" }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Credential link")).toHaveValue(
      `${window.location.origin}/set-password#test-link`,
    );
    expect(screen.getByText(/If it's lost, create a new link/)).toBeInTheDocument();
    const clipboard = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
    await user.click(screen.getByRole("button", { name: "Copy link" }));
    expect(clipboard).toHaveBeenCalledWith(`${window.location.origin}/set-password#test-link`);
    expect(toast.success).toHaveBeenCalledWith("Link copied.");
    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(screen.queryByLabelText("Credential link")).not.toBeInTheDocument();
  });

  test("create-user dialog blocks a blank display name", async () => {
    const create = vi.spyOn(usersApi, "createUser");
    renderUsersPage(ADMIN_ME, []);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Create user" }));
    await user.type(screen.getByLabelText("Email"), "carol@example.com");
    await user.click(screen.getByRole("button", { name: "Create user" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Enter a display name.");
    expect(create).not.toHaveBeenCalled();
  });

  test.each(["unavailable", "denied"])(
    "copy gives manual guidance when clipboard is %s",
    async (mode) => {
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
      if (mode === "unavailable") {
        Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
      } else {
        vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(
          new Error("Permission denied"),
        );
      }
      try {
        await user.click(copy);
        expect(toast.error).toHaveBeenCalledWith(
          "Couldn't copy the link. Select the link and copy it manually.",
        );
        expect(screen.getByLabelText("Credential link")).toHaveValue(
          `${window.location.origin}/set-password#test-link`,
        );
        expect(toast.success).not.toHaveBeenCalled();
      } finally {
        if (descriptor) Object.defineProperty(navigator, "clipboard", descriptor);
      }
    },
  );

  test("reset confirmation replaces a pending link and returns the new link", async () => {
    const target = makeUser({ credential_link_expires_at: "2026-10-23T12:00:00Z" });
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const issue = vi
      .spyOn(usersApi, "issueCredentialLink")
      .mockResolvedValue({ token: "replacement", expires_at: "2026-10-24T12:00:00Z" });
    renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Reset password" }));
    expect(screen.getByRole("heading", { name: "Reset password for Bob?" })).toBeInTheDocument();
    expect(screen.getByText(/This replaces the link created earlier/)).toHaveTextContent(
      "Their current password keeps working until the link is used.",
    );
    expect(screen.queryByLabelText("New password")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create link" }));
    expect(issue).toHaveBeenCalledWith(target.id);
    expect(
      await screen.findByRole("heading", { name: "Send this link to Bob" }),
    ).toBeInTheDocument();
    expect(screen.getByText(/uses this link to choose a new password/)).toBeInTheDocument();
  });

  test("reports the no-email refusal", async () => {
    vi.spyOn(usersApi, "issueCredentialLink").mockRejectedValue(
      new ApiError(422, null, "Validation failed", "the account has no email address"),
    );
    renderUsersPage(ADMIN_ME, [makeUser()]);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Reset password" }));
    await user.click(screen.getByRole("button", { name: "Create link" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Add an email address to Bob before creating a link.",
    );
  });

  test.each([false, true])(
    "shows the pending badge and cancels a link (has password: %s)",
    async (hasPassword) => {
      const target = makeUser({
        has_password: hasPassword,
        credential_link_expires_at: "2026-10-23T12:00:00Z",
      });
      const list = vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
      const cancel = vi.spyOn(usersApi, "cancelCredentialLink").mockImplementation(() => {
        list.mockResolvedValue([{ ...target, credential_link_expires_at: null }]);
        return Promise.resolve();
      });
      renderUsersPage(ADMIN_ME, [target]);
      const user = userEvent.setup();
      const label = hasPassword ? "link pending" : "invited";
      expect(await screen.findByText(label)).toHaveAccessibleName(new RegExp(`${label} expires`));
      await user.click(await screen.findByRole("button", { name: "Cancel link" }));
      expect(cancel).toHaveBeenCalledWith(target.id);
      await waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith("Link cancelled.");
      });
    },
  );

  test("hides reset on the current administrator's row", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id })]);
    await screen.findByText("Bob");
    expect(screen.queryByRole("button", { name: "Reset password" })).not.toBeInTheDocument();
  });

  test("hides all credential actions when local authentication is disabled", async () => {
    vi.mocked(fetchSetupStatus).mockResolvedValueOnce({
      setup_required: false,
      local_auth_enabled: false,
      oidc_enabled: true,
      password_min_length: 15,
      password_max_length: 256,
    });
    renderUsersPage(ADMIN_ME, [makeUser({ credential_link_expires_at: "2026-10-23T12:00:00Z" })]);
    await screen.findByText("Bob");
    expect(screen.queryByRole("button", { name: "Create user" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Reset password" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel link" })).not.toBeInTheDocument();
  });
});
