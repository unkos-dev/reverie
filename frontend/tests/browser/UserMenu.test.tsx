import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
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
    await renderChip();
    await userEvent.click(page.getByRole("button", { name: /Ada Lovelace/ }));
    await expect.element(page.getByRole("menu")).toBeInTheDocument();
    await expect
      .element(page.getByRole("menuitem", { name: "Change password" }))
      .not.toBeInTheDocument();
  });

  test("renders nothing while authn is pending or logged out", async () => {
    useAuthMeMock.mockReturnValue({ data: undefined, isLoading: true, isError: false });
    const { container } = await renderChip();
    await expect.element(container).toBeEmptyDOMElement();
  });

  test("chip shows initials and opens the menu with account identity", async () => {
    await renderChip();
    const user = userEvent;
    const chip = page.getByRole("button", { name: /Ada Lovelace/ });
    await expect.element(chip).toMatchTextContent("AL");
    await user.click(chip);
    await expect.element(page.getByRole("menu")).toBeInTheDocument();
    await expect.element(page.getByText("ada@example.com")).toBeInTheDocument();
  });

  test("an administrator gets a Settings link to the settings area", async () => {
    const me = useAuthMeMock().data;
    if (me === undefined) throw new Error("fixture missing");
    useAuthMeMock.mockReturnValue({
      data: { ...me, role: "admin" },
      isLoading: false,
      isError: false,
    });
    await renderChip();
    await userEvent.click(page.getByRole("button", { name: /Ada Lovelace/ }));
    const settings = page.getByRole("menuitem", { name: "Settings" });
    await expect.element(settings).toHaveAttribute("href", "/admin/settings");
    await expect.element(settings).not.toHaveAttribute("aria-disabled");
  });

  test("a non-admin sees no Settings item and no planned hint", async () => {
    await renderChip();
    await userEvent.click(page.getByRole("button", { name: /Ada Lovelace/ }));
    await expect.element(page.getByRole("menu")).toBeInTheDocument();
    await expect.element(page.getByRole("menuitem", { name: /Settings/ })).not.toBeInTheDocument();
    await expect.element(page.getByText("planned")).not.toBeInTheDocument();
  });

  test("exposes a Change password link to the account screen", async () => {
    await renderChip();
    const user = userEvent;
    await user.click(page.getByRole("button", { name: /Ada Lovelace/ }));
    const link = page.getByRole("menuitem", { name: "Change password" });
    await expect.element(link).toHaveAttribute("href", "/account/password");
  });

  test.each(["pointer", "keyboard"])(
    "theme submenu exposes the three preferences and writes through with %s",
    async (input) => {
      await renderChip();
      const user = userEvent;
      await user.click(page.getByRole("button", { name: /Ada Lovelace/ }));
      if (input === "pointer") {
        await user.click(page.getByRole("menuitem", { name: "Theme" }));
      } else {
        await user.keyboard("{End}{ArrowUp}{ArrowRight}{End}");
      }
      const dark = page.getByRole("menuitemradio", { name: "Dark" });
      await expect.element(page.getByRole("menuitemradio", { name: "System" })).toBeInTheDocument();
      await expect.element(page.getByRole("menuitemradio", { name: "Light" })).toBeInTheDocument();
      if (input === "pointer") {
        await user.click(dark);
      } else {
        await expect.element(dark).toHaveFocus();
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
  await renderChip();
  await userEvent.click(page.getByRole("button", { name: /Ada Lovelace/ }));
  await expect
    .element(page.getByRole("menuitem", { name: "Email address" }))
    .toHaveAttribute("href", "/account/email");
  await expect
    .element(page.getByRole("menuitem", { name: "Change password" }))
    .not.toBeInTheDocument();
});
