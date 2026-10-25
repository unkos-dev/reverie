import { render, screen } from "@testing-library/react";
import { userEvent } from "vite-plus/test/browser";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";
import { MemoryRouter } from "react-router";

import { logout } from "@/api/auth";
import { useAuthMe } from "@/hooks/useAuthMe";

import { UserChip } from "@/components/shell/UserMenu";

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

describe("UserChip", () => {
  test("hides Change password for an OIDC-only account", async () => {
    const me = useAuthMeMock().data;
    if (me === undefined) throw new Error("fixture missing");
    useAuthMeMock.mockReturnValue({
      data: { ...me, has_local_password: false },
      isLoading: false,
      isError: false,
    });
    renderChip();
    await userEvent.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Change password" })).not.toBeInTheDocument();
  });

  test("renders nothing while authn is pending or logged out", () => {
    useAuthMeMock.mockReturnValue({ data: undefined, isLoading: true, isError: false });
    const { container } = renderChip();
    expect(container).toBeEmptyDOMElement();
  });

  test("chip shows initials and opens the menu with account identity", async () => {
    renderChip();
    const user = userEvent;
    const chip = screen.getByRole("button", { name: /Ada Lovelace/ });
    expect(chip).toMatchTextContent("AL");
    await user.click(chip);
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    expect(screen.getByText("ada@example.com")).toBeInTheDocument();
  });

  test("an administrator gets a Settings link to the settings area", async () => {
    const me = useAuthMeMock().data;
    if (me === undefined) throw new Error("fixture missing");
    useAuthMeMock.mockReturnValue({
      data: { ...me, role: "admin" },
      isLoading: false,
      isError: false,
    });
    renderChip();
    await userEvent.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    const settings = await screen.findByRole("menuitem", { name: "Settings" });
    expect(settings).toHaveAttribute("href", "/admin/settings");
    expect(settings).not.toHaveAttribute("aria-disabled");
  });

  test("a non-admin sees no Settings item and no planned hint", async () => {
    renderChip();
    await userEvent.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Settings/ })).not.toBeInTheDocument();
    expect(screen.queryByText("planned")).not.toBeInTheDocument();
  });

  test("exposes a Change password link to the account screen", async () => {
    renderChip();
    const user = userEvent;
    await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
    const link = await screen.findByRole("menuitem", { name: "Change password" });
    expect(link).toHaveAttribute("href", "/account/password");
  });

  test.each(["pointer", "keyboard"])(
    "theme submenu exposes the three preferences and writes through with %s",
    async (input) => {
      renderChip();
      const user = userEvent;
      await user.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
      if (input === "pointer") {
        await user.click(await screen.findByRole("menuitem", { name: "Theme" }));
      } else {
        await user.keyboard("{End}{ArrowUp}{ArrowRight}{End}");
      }
      const dark = await screen.findByRole("menuitemradio", { name: "Dark" });
      expect(screen.getByRole("menuitemradio", { name: "System" })).toBeInTheDocument();
      expect(screen.getByRole("menuitemradio", { name: "Light" })).toBeInTheDocument();
      if (input === "pointer") {
        await user.click(dark);
      } else {
        expect(dark).toHaveFocus();
        await user.keyboard("{Enter}");
      }
      expect(setPreferenceMock).toHaveBeenCalledWith("dark");
    },
  );
});
test("OIDC-only accounts can open email settings", async () => {
  const me = useAuthMeMock().data;
  if (me === undefined) throw new Error("fixture missing");
  useAuthMeMock.mockReturnValue({
    data: { ...me, has_local_password: false },
    isLoading: false,
    isError: false,
  });
  renderChip();
  await userEvent.click(screen.getByRole("button", { name: /Ada Lovelace/ }));
  expect(await screen.findByRole("menuitem", { name: "Email address" })).toHaveAttribute(
    "href",
    "/account/email",
  );
  expect(screen.queryByRole("menuitem", { name: "Change password" })).not.toBeInTheDocument();
});
