import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { MemoryRouter } from "react-router";

import { logout } from "@/api/auth";
import { useAuthMe } from "@/hooks/useAuthMe";
import { activeUserId, rememberActiveUser } from "@/lib/active-user";

import { UserChip } from "./UserMenu";

function renderChip(): ReturnType<typeof render> {
  return render(
    <MemoryRouter>
      <UserChip />
    </MemoryRouter>,
  );
}

vi.mock("@/api/auth", () => ({ logout: vi.fn() }));
vi.mock("@/hooks/useAuthMe", () => ({ useAuthMe: vi.fn() }));

const setPreferenceMock = vi.fn();
vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({
    preference: "system",
    effective: "dark",
    setPreference: setPreferenceMock,
  }),
}));

const logoutMock = vi.mocked(logout);
const useAuthMeMock = vi.mocked(useAuthMe);

const originalLocation = window.location;

function mockLocation(): { assign: ReturnType<typeof vi.fn> } {
  const loc = { assign: vi.fn(), href: "http://localhost/" };
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: loc,
  });
  return loc;
}

beforeEach(() => {
  useAuthMeMock.mockReturnValue({
    data: {
      id: "u-1",
      display_name: "Ada Lovelace",
      email: "ada@example.com",
      role: "adult",
      is_child: false,
      has_local_password: true,
      theme_preference: "system",
      csrf_token: null,
    },
    isLoading: false,
    isError: false,
  });
  logoutMock.mockResolvedValue(undefined);
});

afterEach(() => {
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: originalLocation,
  });
  vi.clearAllMocks();
});

describe("UserChip", () => {
  test("sign out calls the logout endpoint then hard-navigates to /login", async () => {
    const loc = mockLocation();
    renderChip();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    await user.click(await screen.findByRole("menuitem", { name: /Sign out/ }));
    await waitFor(() => {
      expect(logoutMock).toHaveBeenCalledTimes(1);
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });
  });

  test("sign out forgets the browser's active account", async () => {
    // Per-user caches (the library's first-paint mirror) key off this id;
    // forgetting it is what keeps one account's cached presentation from
    // seeding the next sign-in on a shared device.
    rememberActiveUser("user-a");
    const loc = mockLocation();
    renderChip();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    await user.click(await screen.findByRole("menuitem", { name: /Sign out/ }));
    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });
    expect(activeUserId()).toBeNull();
  });

  test("still navigates to /login when logout fails", async () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => undefined);
    logoutMock.mockRejectedValue(new Error("network down"));
    const loc = mockLocation();
    renderChip();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    await user.click(await screen.findByRole("menuitem", { name: /Sign out/ }));
    await waitFor(() => {
      expect(loc.assign).toHaveBeenCalledWith("/login");
    });
    expect(consoleSpy).toHaveBeenCalled();
  });
});
