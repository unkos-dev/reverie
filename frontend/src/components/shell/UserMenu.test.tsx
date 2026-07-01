import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { MemoryRouter } from "react-router";

import { logout } from "@/api/auth";
import { useAuthMe } from "@/hooks/useAuthMe";

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
  test("renders nothing while authn is pending or logged out", () => {
    useAuthMeMock.mockReturnValue({ data: undefined, isLoading: true, isError: false });
    const { container } = renderChip();
    expect(container).toBeEmptyDOMElement();
  });

  test("chip shows initials and opens the menu with account identity", async () => {
    renderChip();
    const user = userEvent.setup();
    const chip = screen.getByRole("button", { name: /Ada Lovelace/ });
    expect(chip).toHaveTextContent("AL");
    await user.click(chip);
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    expect(screen.getByText("ada@example.com")).toBeInTheDocument();
  });

  test("Settings item is disabled", async () => {
    renderChip();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    const settings = await screen.findByRole("menuitem", { name: /Settings/ });
    expect(settings).toHaveAttribute("aria-disabled", "true");
  });

  test("exposes a Change password link to the account screen", async () => {
    renderChip();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    const link = await screen.findByRole("menuitem", { name: "Change password" });
    expect(link).toHaveAttribute("href", "/account/password");
  });

  test("theme submenu exposes the three preferences and writes through", async () => {
    renderChip();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    await user.click(await screen.findByRole("menuitem", { name: "Theme" }));
    const dark = await screen.findByRole("menuitemradio", { name: "Dark" });
    expect(screen.getByRole("menuitemradio", { name: "System" })).toBeInTheDocument();
    expect(screen.getByRole("menuitemradio", { name: "Light" })).toBeInTheDocument();
    // Keyboard activation — Radix's pointer-intent tracking swallows
    // synthetic jsdom clicks inside submenus; Enter drives the same
    // onSelect path a real pointer does.
    dark.focus();
    await user.keyboard("{Enter}");
    expect(setPreferenceMock).toHaveBeenCalledWith("dark");
  });

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
