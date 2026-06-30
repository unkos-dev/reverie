import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { User } from "@/api/users";
import * as usersApi from "@/api/users";

import { UsersPage } from "./UsersPage";

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

function makeUser(overrides: Partial<User> = {}): User {
  return {
    id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
    display_name: "Bob",
    email: null,
    role: "adult",
    is_child: false,
    disabled: false,
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

  test("offers no disable control for the calling admin's own row", async () => {
    renderUsersPage(ADMIN_ME, [makeUser({ id: ADMIN_ME.id, display_name: "Alice" })]);
    await screen.findByText("Alice");
    expect(screen.queryByRole("button", { name: "Disable" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Enable" })).not.toBeInTheDocument();
  });

  test("create-user dialog submits the new account", async () => {
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([]);
    const create = vi
      .spyOn(usersApi, "createUser")
      .mockResolvedValue(makeUser({ display_name: "Carol", email: "carol@example.com" }));
    renderUsersPage(ADMIN_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Create user" }));
    await user.type(screen.getByLabelText("Display name"), "Carol");
    await user.type(screen.getByLabelText("Email"), "carol@example.com");
    await user.type(screen.getByLabelText("Initial password"), "correct-horse-battery");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(create).toHaveBeenCalledWith({
      email: "carol@example.com",
      display_name: "Carol",
      role: "adult",
      password: "correct-horse-battery",
    });
  });

  test("create-user dialog blocks a blank display name without calling the API", async () => {
    const create = vi.spyOn(usersApi, "createUser");
    renderUsersPage(ADMIN_ME, []);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Create user" }));
    await user.type(screen.getByLabelText("Email"), "carol@example.com");
    await user.type(screen.getByLabelText("Initial password"), "correct-horse-battery");
    await user.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(create).not.toHaveBeenCalled();
  });

  test("reset-password dialog calls adminResetPassword", async () => {
    const target = makeUser();
    vi.spyOn(usersApi, "listUsers").mockResolvedValue([target]);
    const reset = vi.spyOn(usersApi, "adminResetPassword").mockResolvedValue(undefined);
    renderUsersPage(ADMIN_ME, [target]);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Reset password" }));
    await user.type(screen.getByLabelText("New password"), "correct-horse-battery");
    await user.click(screen.getByRole("button", { name: "Reset" }));

    expect(reset).toHaveBeenCalledWith(target.id, "correct-horse-battery");
  });
});
