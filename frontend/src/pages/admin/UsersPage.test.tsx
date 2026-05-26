import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { RouterProvider, createMemoryRouter, type RouteObject } from "react-router";
import type { ReactElement } from "react";

import { queryKeys } from "@/lib/query/keys";
import type { User } from "@/api/users";
import * as usersApi from "@/api/users";

import { UsersPage } from "./UsersPage";

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
});
