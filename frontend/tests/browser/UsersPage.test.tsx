import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent, cdp } from "vite-plus/test/browser";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { Role, User } from "@/api/users";
import * as usersApi from "@/api/users";
import { toast } from "sonner";
import { ApiError } from "@/api";
import { fetchSetupStatus, fetchEmailVerification } from "@/api/auth";

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

afterEach(async () => {
  await cleanup();
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

async function renderUsersPage(
  meData: typeof ADMIN_ME | typeof ADULT_ME | null,
  users: User[] | null = null,
): Promise<QueryClient> {
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

  await render(<Wrapper />);
  if (meData?.role === "admin")
    await expect.element(page.getByRole("heading", { name: "Users" })).toBeInTheDocument();
  return client;
}

describe("UsersPage", () => {
  test("redirects non-admin to /library", async () => {
    await renderUsersPage(ADULT_ME, []);
    await expect.element(page.getByTestId("library-page")).toBeInTheDocument();
  });

  test("shows empty state when users list is empty", async () => {
    await renderUsersPage(ADMIN_ME, []);
    await expect.element(page.getByText(/no users found/i)).toBeInTheDocument();
  });

  test("renders user rows when list has users", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser()]);
    await expect.element(page.getByText("Bob")).toBeInTheDocument();
  });

  test("marks self with 'you' badge", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id, display_name: "Alice" })]);
    await expect.element(page.getByText("you")).toBeInTheDocument();
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

    await render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    await expect.element(page.getByText(/failed to load users/i)).toBeInTheDocument();
  });

  test("shows table with correct columns when users present", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser({ email: "bob@example.com" })]);
    await expect.element(page.getByText("Bob")).toBeInTheDocument();
    await expect.element(page.getByText("bob@example.com")).toBeInTheDocument();
    expect(
      page
        .getByRole("columnheader")
        .elements()
        .map((header) => header.textContent),
    ).toEqual(["Name", "Email", "Role", "Status", "Actions"]);
    await expect.element(page.getByRole("switch")).not.toBeInTheDocument();
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
    const client = await renderUsersPage(ADMIN_ME, [target]);
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const user = userEvent;

    await user.click(page.getByRole("button", { name: /Role/ }));
    vi.mocked(usersApi.listUsers).mockResolvedValue([updated]);
    await user.click(page.getByRole("option", { name: to }));

    await vi.waitFor(() => {
      expect(changeRole).toHaveBeenCalledWith(target.id, to);
    });
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("button", { name: /Role/ })).toMatchTextContent(to);
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
    await renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: /Role/ }));
    await user.click(page.getByRole("option", { name: "child" }));

    await vi.waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Role update failed: would leave zero admins");
    });
    await expect.element(page.getByRole("button", { name: /Role/ })).toMatchTextContent("admin");
  });

  test("renders a disabled badge for a disabled account", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser({ disabled: true })]);
    await expect.element(page.getByText("disabled")).toBeInTheDocument();
  });

  test("renders an active badge for an enabled account", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser()]);
    await expect.element(page.getByText("active")).toBeInTheDocument();
  });

  test("disable button calls setAccountStatus with disabled=true", async () => {
    const target = makeUser();
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const setStatus = vi
      .spyOn(usersApi, "setAccountStatus")
      .mockResolvedValue({ ...target, disabled: true });
    await renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "Disable" }));
    expect(setStatus).toHaveBeenCalledWith(target.id, true);
  });

  test("shows an Enable control for a disabled account", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser({ disabled: true })]);
    await expect.element(page.getByRole("button", { name: "Enable" })).toBeInTheDocument();
  });

  test("enable button calls setAccountStatus with disabled=false", async () => {
    const target = makeUser({ disabled: true });
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const setStatus = vi
      .spyOn(usersApi, "setAccountStatus")
      .mockResolvedValue({ ...target, disabled: false });
    await renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent;

    await user.click(page.getByRole("button", { name: "Enable" }));
    expect(setStatus).toHaveBeenCalledWith(target.id, false);
  });

  test("offers no disable control for the calling admin's own row", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id, display_name: "Alice" })]);
    await expect.element(page.getByText("Alice", { exact: false })).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Disable" })).not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Enable" })).not.toBeInTheDocument();
  });

  test("create-user dialog returns a link without accepting a password", async () => {
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([]);
    const create = vi.spyOn(usersApi, "createUser").mockResolvedValue({
      user: makeUser({ display_name: "Carol", email: "carol@example.com", has_password: false }),
      link: { token: "test-link", expires_at: "2026-10-23T12:00:00Z" },
    });
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    const dialog = page.getByRole("dialog", { name: "Create user" }).element();
    await expect.element(page.getByLabelText(/password/i)).not.toBeInTheDocument();
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
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

  test("native role values retain a failed choice and restore the default on reopening", async () => {
    const create = vi
      .spyOn(usersApi, "createUser")
      .mockRejectedValue(new Error("Creation refused"));
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    const form = page.getByLabelText("Display name").element().closest("form");
    if (!form) throw new Error("Create-user form missing");
    expect(new FormData(form).get("role")).toBe("adult");
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: /Role/ }));
    await user.click(page.getByRole("option", { name: "child" }));
    expect(new FormData(form).get("role")).toBe("child");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await expect.element(page.getByRole("alert")).toMatchTextContent("Creation refused");
    expect(create).toHaveBeenCalledWith({
      display_name: "Carol",
      email: "carol@example.com",
      role: "child",
    });
    expect(new FormData(form).get("role")).toBe("child");
    await expect.element(page.getByRole("button", { name: /Role/ })).toMatchTextContent("child");
    await user.click(page.getByRole("button", { name: "Cancel" }));
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    const reopened = page.getByLabelText("Display name").element().closest("form");
    if (!reopened) throw new Error("Reopened form missing");
    expect(new FormData(reopened).get("role")).toBe("adult");
    await expect.element(page.getByRole("button", { name: /Role/ })).toMatchTextContent("adult");
  });

  test.each(["admin", "child"])("submits the native %s role", async (role) => {
    const create = vi
      .spyOn(usersApi, "createUser")
      .mockRejectedValue(new Error("Creation refused"));
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: /Role/ }));
    await user.click(page.getByRole("option", { name: role }));
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    expect(create).toHaveBeenCalledWith({
      display_name: "Carol",
      email: "carol@example.com",
      role,
    });
    await expect.element(page.getByRole("alert")).toBeInTheDocument();
  });

  test.each([null, "", "owner"])(
    "rejects an invalid submitted role (%s) before mutation",
    async (role) => {
      const create = vi.spyOn(usersApi, "createUser");
      await renderUsersPage(ADMIN_ME, []);
      const user = userEvent;
      await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
      const form = page.getByLabelText("Display name").element().closest("form");
      if (!form) throw new Error("Create-user form missing");
      const select = form.querySelector('select[name="role"]');
      if (!(select instanceof HTMLSelectElement)) throw new Error("Named Select missing");
      if (role === null) select.removeAttribute("name");
      else {
        select.add(new Option(role, role));
        select.value = role;
      }
      await user.type(page.getByLabelText("Display name"), "Carol");
      await user.type(page.getByLabelText("Email"), "carol@example.com");
      form.requestSubmit();
      await expect.element(page.getByRole("alert")).toMatchTextContent("Choose a valid role.");
      expect(create).not.toHaveBeenCalled();
    },
  );

  test("native validation owns creation errors", async () => {
    const create = vi.spyOn(usersApi, "createUser");
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    const displayName = page.getByLabelText("Display name");
    const email = page.getByLabelText("Email");
    await user.type(displayName, "   ");
    await user.type(email, "not-an-email");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });

    expect(create).not.toHaveBeenCalled();
    await expect.element(displayName).toHaveAccessibleDescription("Enter a display name.");
    await expect.element(email).toHaveAccessibleDescription("Enter a valid email address.");
    await expect.element(displayName).toBeInvalid();
    await expect.element(email).toBeInvalid();
    await expect.element(displayName).toHaveFocus();
    await expect.element(page.getByRole("alert")).not.toBeInTheDocument();
  });

  test("create-user dialog blocks a blank display name", async () => {
    const create = vi.spyOn(usersApi, "createUser");
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await expect
      .element(page.getByLabelText("Display name"))
      .toHaveAccessibleDescription("Enter a display name.");
    await expect.element(page.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
    expect(create).not.toHaveBeenCalled();
  });

  test("a refused creation is a general alert that keeps the values without invalid fields", async () => {
    vi.spyOn(usersApi, "createUser").mockRejectedValue(new Error("Creation refused"));
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });

    await expect.element(page.getByRole("alert")).toMatchTextContent("Creation refused");
    await expect.element(page.getByLabelText("Display name")).toHaveValue("Carol");
    await expect.element(page.getByLabelText("Email")).toHaveValue("carol@example.com");
    await expect.element(page.getByLabelText("Display name")).not.toHaveAttribute("aria-invalid");
    await expect.element(page.getByLabelText("Email")).not.toHaveAttribute("aria-invalid");
  });

  test("repeated click and Enter while creation is pending send one request", async () => {
    let reject: (error: Error) => void = () => {};
    const create = vi.spyOn(usersApi, "createUser").mockReturnValue(
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
    );
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.keyboard("{Enter}");
    await user.click(page.getByLabelText("Email"));
    await user.keyboard("{Enter}");

    expect(create).toHaveBeenCalledTimes(1);
    reject(new Error("Creation refused"));
    await expect
      .element(page.getByRole("button", { name: "Create user" }).last())
      .not.toHaveAttribute("data-pending");
  });

  test("reopening during a deferred creation shows default values and cannot send a second request", async () => {
    let reject: (error: Error) => void = () => {};
    const create = vi.spyOn(usersApi, "createUser").mockReturnValue(
      new Promise((_resolve, fail) => {
        reject = fail;
      }),
    );
    await renderUsersPage(ADMIN_ME, []);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.type(page.getByLabelText("Display name"), "Carol");
    await user.type(page.getByLabelText("Email"), "carol@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    await user.click(page.getByRole("button", { name: "Cancel" }));
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });

    await expect.element(page.getByLabelText("Display name")).toHaveValue("");
    await expect.element(page.getByRole("button", { name: /Role/ })).toMatchTextContent("adult");
    await user.type(page.getByLabelText("Display name"), "Dave");
    await user.type(page.getByLabelText("Email"), "dave@example.com");
    await user.click(page.getByRole("button", { name: "Create user" }).last(), { force: true });
    expect(create).toHaveBeenCalledTimes(1);
    reject(new Error("Creation refused"));
    await expect
      .element(page.getByRole("button", { name: "Create user" }).last())
      .not.toHaveAttribute("data-pending");
  });

  test("copy gives manual guidance when clipboard permission is denied", async () => {
    const target = makeUser();
    vi.spyOn(usersApi, "issueCredentialLink").mockResolvedValue({
      token: "test-link",
      expires_at: "2026-10-23T12:00:00Z",
    });
    await renderUsersPage(ADMIN_ME, [target]);
    await userEvent.click(page.getByRole("button", { name: "Reset password" }));
    await userEvent.click(page.getByRole("button", { name: "Create link" }));
    const { targetInfo } = await cdp().send("Target.getTargetInfo");
    await cdp().send("Browser.setPermission", {
      browserContextId: targetInfo.browserContextId,
      origin: window.location.origin,
      permission: { name: "clipboard-write" },
      setting: "denied",
    });
    await userEvent.click(page.getByRole("button", { name: "Copy link" }));
    await vi.waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "Couldn't copy the link. Select the link and copy it manually.",
      );
    });
    await expect
      .element(page.getByLabelText("Credential link"))
      .toHaveValue(`${window.location.origin}/reset-password#test-link`);
    expect(toast.success).not.toHaveBeenCalled();
  });

  test("reset confirmation replaces a pending link and returns the new link", async () => {
    const target = makeUser({ credential_link_expires_at: "2026-10-23T12:00:00Z" });
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const issue = vi
      .spyOn(usersApi, "issueCredentialLink")
      .mockResolvedValue({ token: "replacement", expires_at: "2026-10-24T12:00:00Z" });
    await renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Reset password" }));
    await expect
      .element(page.getByRole("heading", { name: "Reset password for Bob?" }))
      .toBeInTheDocument();
    await expect
      .element(page.getByText(/This replaces the link created earlier/))
      .toMatchTextContent("Their current password keeps working until the link is used.");
    await expect.element(page.getByLabelText("New password")).not.toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Create link" }));
    expect(issue).toHaveBeenCalledWith(target.id);
    await expect
      .element(page.getByRole("heading", { name: "Send this link to Bob" }))
      .toBeInTheDocument();
    await expect
      .element(page.getByText(/uses this link to choose a new password/))
      .toBeInTheDocument();
  });

  test("reports the no-email refusal", async () => {
    vi.spyOn(usersApi, "issueCredentialLink").mockRejectedValue(
      new ApiError(422, null, "Validation failed", "the account has no email address"),
    );
    await renderUsersPage(ADMIN_ME, [makeUser()]);
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Reset password" }));
    await user.click(page.getByRole("button", { name: "Create link" }));
    await expect
      .element(page.getByRole("alert"))
      .toMatchTextContent("Add an email address to Bob before creating a link.");
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
      await renderUsersPage(ADMIN_ME, [target]);
      const user = userEvent;
      const label = hasPassword ? "link pending" : "invited";
      await expect
        .element(page.getByText(label))
        .toHaveAccessibleName(new RegExp(`${label} expires`));
      await user.click(page.getByRole("button", { name: "Cancel link" }));
      expect(cancel).toHaveBeenCalledWith(target.id);
      await vi.waitFor(() => {
        expect(toast.success).toHaveBeenCalledWith("Link cancelled.");
      });
    },
  );

  test("hides reset on the current administrator's row", async () => {
    await renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id })]);
    await expect.element(page.getByText("Bob", { exact: false })).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Reset password" }))
      .not.toBeInTheDocument();
  });

  test("hides all credential actions when local authentication is disabled", async () => {
    vi.mocked(fetchSetupStatus).mockResolvedValueOnce({
      setup_required: false,
      local_auth_enabled: false,
      oidc_enabled: true,
      email_recovery_enabled: false,
      password_min_length: 15,
      password_max_length: 256,
    });
    await renderUsersPage(ADMIN_ME, [
      makeUser({ credential_link_expires_at: "2026-10-23T12:00:00Z" }),
    ]);
    await expect.element(page.getByText("Bob")).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Create user" }).last())
      .not.toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Reset password" }))
      .not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Cancel link" })).not.toBeInTheDocument();
  });

  test("shows authentication settings loading until credential actions are known", async () => {
    vi.mocked(fetchSetupStatus).mockReturnValueOnce(new Promise(() => {}));
    await renderUsersPage(ADMIN_ME, [makeUser()]);
    await expect
      .element(page.getByRole("status"))
      .toMatchTextContent("Loading authentication settings…");
    await expect
      .element(page.getByRole("button", { name: "Create user" }).last())
      .not.toBeInTheDocument();
    await expect.element(page.getByText("Bob")).toBeInTheDocument();
  });

  test.each([new TypeError("Failed to fetch"), new ApiError(503, null, "Busy", "Try later")])(
    "recovers credential actions in place after authentication settings fail: %s",
    async (error) => {
      vi.mocked(fetchSetupStatus).mockClear();
      vi.mocked(fetchSetupStatus).mockRejectedValueOnce(error);
      const target = makeUser({ credential_link_expires_at: "2026-10-23T12:00:00Z" });
      await renderUsersPage(ADMIN_ME, [target]);
      const retry = page.getByRole("button", { name: "Retry authentication settings" });
      await expect
        .element(page.getByRole("alert"))
        .toMatchTextContent("Could not load authentication settings.");
      await expect
        .element(page.getByRole("button", { name: "Create user" }).last())
        .not.toBeInTheDocument();
      await expect
        .element(page.getByRole("button", { name: "Reset password" }))
        .not.toBeInTheDocument();
      await expect
        .element(page.getByRole("button", { name: "Cancel link" }))
        .not.toBeInTheDocument();
      await expect.element(page.getByRole("button", { name: "Disable" })).toBeEnabled();
      const status = {
        setup_required: false,
        local_auth_enabled: true,
        oidc_enabled: false,
        email_recovery_enabled: false,
        password_min_length: 15,
        password_max_length: 256,
      };
      let complete: ((value: typeof status) => void) | undefined;
      vi.mocked(fetchSetupStatus).mockReturnValueOnce(
        new Promise((resolve) => {
          complete = resolve;
        }),
      );
      await userEvent.click(retry);
      await expect.element(retry).toBeDisabled();
      {
        complete?.(status);
      }
      await expect.element(page.getByRole("button", { name: "Create user" }).last()).toBeEnabled();
      await expect.element(page.getByRole("button", { name: "Reset password" })).toBeEnabled();
      await expect.element(page.getByRole("button", { name: "Cancel link" })).toBeEnabled();
      await expect
        .element(page.getByRole("button", { name: "Retry authentication settings" }))
        .not.toBeInTheDocument();
      expect(fetchSetupStatus).toHaveBeenCalledTimes(2);
    },
  );
});

const mailJob = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const mailToken = "A".repeat(43);
async function openMailableLink(): Promise<HTMLElement | SVGElement> {
  vi.mocked(fetchEmailVerification).mockResolvedValue({
    email: "alice@example.com",
    email_verified: false,
    mail_enabled: true,
  });
  vi.spyOn(usersApi, "issueCredentialLink").mockResolvedValue({
    token: mailToken,
    expires_at: "2026-10-23T12:00:00Z",
  });
  await renderUsersPage(ADMIN_ME, [makeUser({ email: "bob@example.com" })]);
  const user = userEvent;
  await user.click(page.getByRole("button", { name: "Reset password" }));
  const dialog = page.getByRole("dialog", { name: "Reset password for Bob?" }).element();
  await user.click(page.getByRole("button", { name: "Create link" }));
  await expect.element(page.getByRole("button", { name: "Send email" })).toBeInTheDocument();
  return dialog;
}

test("keeps focus in the link result and returns it to reset after Escape", async () => {
  const original = await openMailableLink();
  const dialog = page.getByRole("dialog", { name: "Send this link to Bob" });
  expect(dialog.element()).toBe(original);
  await vi.waitFor(() => {
    expect(dialog.element().contains(document.activeElement)).toBe(true);
  });
  const user = userEvent;
  for (let i = 0; i < 7; i++) {
    await user.tab();
    expect(dialog.element().contains(document.activeElement)).toBe(true);
  }
  await user.keyboard("{Escape}");
  await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  await vi.waitFor(async () => {
    await expect.element(page.getByRole("button", { name: "Reset password" })).toHaveFocus();
  });
});

test.each(["create", "reset"])(
  "preserves pending %s admission and shows a late successful result",
  async (operation) => {
    const target = makeUser({ email: "bob@example.com" });
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    let complete: ((link: usersApi.CredentialLink) => void) | undefined;
    const pending = new Promise<usersApi.CredentialLink>((resolve) => {
      complete = resolve;
    });
    const create = vi
      .spyOn(usersApi, "createUser")
      .mockReturnValue(
        pending.then((link) => ({ user: { ...target, has_password: false }, link })),
      );
    const issue = vi.spyOn(usersApi, "issueCredentialLink").mockReturnValue(pending);
    await renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent;
    const trigger = operation === "create" ? "Create user" : "Reset password";
    const submit = operation === "create" ? "Create user" : "Create link";
    await user.click(page.getByRole("button", { name: trigger }).first());
    if (operation === "create") {
      await user.type(page.getByLabelText("Display name"), "Bob");
      await user.type(page.getByLabelText("Email"), "bob@example.com");
    }
    await user.click(page.getByRole("button", { name: submit }).last(), { force: true });
    await expect
      .element(page.getByRole("button", { name: submit }).last())
      .toHaveAttribute("aria-disabled", "true");
    await expect.element(page.getByRole("button", { name: submit }).last()).toHaveFocus();
    await user.keyboard("{Enter}");
    await user.click(page.getByRole("button", { name: submit }).last(), { force: true });
    expect(operation === "create" ? create : issue).toHaveBeenCalledTimes(1);
    await user.keyboard("{Escape}");
    await vi.waitFor(async () => {
      await expect.element(page.getByRole("button", { name: trigger }).first()).toHaveFocus();
    });
    await user.keyboard("{Enter}");
    await expect
      .element(page.getByRole("button", { name: submit }).last())
      .toHaveAttribute("aria-disabled", "true");
    await user.keyboard("{Escape}");
    {
      complete?.({ token: mailToken, expires_at: "2026-10-23T12:00:00Z" });
    }
    await expect.element(page.getByRole("dialog", { name: "Send this link to Bob" })).toBeVisible();
    expect(operation === "create" ? create : issue).toHaveBeenCalledTimes(1);
    await user.click(page.getByRole("button", { name: "Done" }));
    await expect.element(page.getByLabelText("Credential link")).not.toBeInTheDocument();
  },
);

test.each([
  [
    { job_id: mailJob, status: "accepted", failure_category: null },
    "The mail server accepted the email for bob@example.com. This doesn't confirm it reached the inbox.",
  ],
  [
    { job_id: mailJob, status: "inactive", failure_category: null },
    "This link is no longer active. Create a new link to send it.",
  ],
  [
    { job_id: mailJob, status: "failed", failure_category: "connection" },
    "Sending failed: couldn't connect to the mail server. Copy the link instead, or try again.",
  ],
] satisfies [usersApi.CredentialLinkEmail, string][])(
  "polls only the displayed delivery until its terminal state %j",
  async (terminal, copy) => {
    const send = vi
      .spyOn(usersApi, "sendCredentialLinkEmail")
      .mockResolvedValue({ job_id: mailJob, status: "queued", failure_category: null });
    const poll = vi.spyOn(usersApi, "fetchCredentialLinkEmail").mockResolvedValue(terminal);
    await openMailableLink();
    expect(send).not.toHaveBeenCalled();
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Send email" }), { force: true });
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 0);
      });
    }
    expect(send).toHaveBeenCalledExactlyOnceWith("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", mailToken);
    await expect
      .element(page.getByRole("button", { name: "Send email" }))
      .toHaveAttribute("aria-disabled", "true");
    await user.keyboard("{Enter}");
    await user.click(page.getByRole("button", { name: "Send email" }), { force: true });
    expect(send).toHaveBeenCalledTimes(1);
    await expect.element(page.getByText("Sending to bob@example.com…")).toBeInTheDocument();
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 3000);
      });
    }
    await expect.element(page.getByText(copy)).toBeInTheDocument();
    expect(poll).toHaveBeenCalledExactlyOnceWith(
      "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
      mailJob,
      expect.any(AbortSignal),
    );
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 3000);
      });
    }
    expect(poll).toHaveBeenCalledTimes(1);
    await expect.element(page.getByRole("button", { name: "Copy link" })).toBeEnabled();
    if (terminal.status === "failed") {
      send.mockResolvedValue({
        job_id: "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
        status: "queued",
        failure_category: null,
      });
      await user.click(page.getByRole("button", { name: "Try again" }));
      {
        await new Promise((resolve) => {
          setTimeout(resolve, 0);
        });
      }
      expect(send).toHaveBeenLastCalledWith("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", mailToken);
      expect(send).toHaveBeenCalledTimes(2);
    } else
      await expect
        .element(page.getByRole("button", { name: "Send email" }))
        .not.toBeInTheDocument();
    await user.click(page.getByRole("button", { name: "Done" }));
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 3000);
      });
    }
    expect(poll).toHaveBeenCalledTimes(1);
    await expect.element(page.getByLabelText("Credential link")).not.toBeInTheDocument();
  },
);

test.each([
  { job_id: mailJob, status: "accepted", failure_category: null },
  { job_id: mailJob, status: "queued", failure_category: null },
] satisfies usersApi.CredentialLinkEmail[])(
  "rechecks the same delivery after a status failure and resumes with %j",
  async (recovered) => {
    const send = vi.spyOn(usersApi, "sendCredentialLinkEmail").mockResolvedValue({
      job_id: mailJob,
      status: "queued",
      failure_category: null,
    });
    const poll = vi
      .spyOn(usersApi, "fetchCredentialLinkEmail")
      .mockRejectedValueOnce(new TypeError("network failure"));
    await openMailableLink();
    const user = userEvent;
    await user.click(page.getByRole("button", { name: "Send email" }), { force: true });
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 3000);
      });
    }
    await expect
      .element(page.getByText("Couldn't confirm the request. Try again."))
      .toBeInTheDocument();
    const retry = page.getByRole("button", { name: "Try again" });
    await expect.element(retry).toBeEnabled();
    await expect.element(retry).not.toHaveAttribute("aria-disabled", "true");
    await expect.element(page.getByRole("button", { name: "Copy link" })).toBeEnabled();
    await expect
      .element(page.getByLabelText("Credential link"))
      .toHaveValue(`${window.location.origin}/reset-password#${mailToken}`);
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 3000);
      });
    }
    expect(poll).toHaveBeenCalledTimes(1);
    let finishRetry: ((value: usersApi.CredentialLinkEmail) => void) | undefined;
    poll.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finishRetry = resolve;
        }),
    );
    poll.mockResolvedValue({ job_id: mailJob, status: "accepted", failure_category: null });
    await user.click(retry);
    await vi.waitFor(() => {
      expect(poll).toHaveBeenCalledTimes(2);
    });
    await expect.element(retry).toHaveAttribute("aria-disabled", "true");
    expect(send).toHaveBeenCalledTimes(1);
    expect(poll).toHaveBeenLastCalledWith(
      "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
      mailJob,
      expect.any(AbortSignal),
    );
    {
      finishRetry?.(recovered);
    }
    {
      await new Promise((resolve) => {
        setTimeout(resolve, 3000);
      });
    }
    await expect
      .element(
        page.getByText(
          "The mail server accepted the email for bob@example.com. This doesn't confirm it reached the inbox.",
        ),
      )
      .toBeInTheDocument();
    expect(poll).toHaveBeenCalledTimes(recovered.status === "queued" ? 3 : 2);
    expect(send).toHaveBeenCalledTimes(1);
    await expect.element(page.getByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  },
);

test.each([
  [
    new ApiError(503, null, "Unavailable", ""),
    "Email isn't available right now. Copy the link instead.",
  ],
  [
    new ApiError(422, null, "Invalid", ""),
    "This link is no longer active. Create a new link to send it.",
  ],
  [new TypeError("network failure"), "Couldn't confirm the request. Try again."],
])("retains manual copying after a send refusal", async (error, copy) => {
  vi.spyOn(usersApi, "sendCredentialLinkEmail").mockRejectedValue(error);
  await openMailableLink();
  const user = userEvent;
  await user.click(page.getByRole("button", { name: "Send email" }), { force: true });
  await expect.element(page.getByText(copy)).toBeInTheDocument();
  await expect.element(page.getByRole("button", { name: "Copy link" })).toBeEnabled();
  await expect
    .element(page.getByLabelText("Credential link"))
    .toHaveValue(`${window.location.origin}/reset-password#${mailToken}`);
});

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

test("a late creation failure keeps the reopened draft and displays its feedback", async () => {
  let reject: (error: Error) => void = () => {};
  vi.mocked(usersApi.createUser).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    }),
  );
  await renderUsersPage(ADMIN_ME, []);
  await userEvent.click(page.getByRole("button", { name: "Create user" }));
  await userEvent.type(page.getByLabelText("Display name"), "Carol");
  await userEvent.type(page.getByLabelText("Email"), "carol@example.com");
  await userEvent.click(page.getByRole("dialog").getByRole("button", { name: "Create user" }));
  await userEvent.keyboard("{Escape}");
  await userEvent.click(page.getByRole("button", { name: "Create user" }));
  await userEvent.type(page.getByLabelText("Display name"), "Dave");
  reject(new Error("Creation refused"));
  await expect.element(page.getByRole("alert")).toMatchTextContent("Creation refused");
  await expect.element(page.getByLabelText("Display name")).toHaveValue("Dave");
  await expect
    .element(page.getByRole("dialog").getByRole("button", { name: "Create user" }))
    .toBeEnabled();
  expect(usersApi.createUser).toHaveBeenCalledTimes(1);
});
