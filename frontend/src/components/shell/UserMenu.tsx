/**
 * User chip + account menu at the bottom of the left rail (spec §3.7).
 *
 * Identity comes from `useAuthMe`; while authn is pending (or the
 * viewer is logged out) the chip renders nothing — the 401 redirect in
 * `App.tsx` owns the logged-out path.
 *
 * THREAT: sign-out must attempt server-side session destruction (POST
 * `/auth/logout`) — dropping UI state alone would leave a replayable
 * session cookie. Navigation is NOT conditional on success: it
 * proceeds even when the request fails so a user on a broken network
 * still lands on the login surface (the cookie's server-side TTL is
 * the backstop); the raw error goes to the console for the operator.
 *
 * Theme switching reuses the `useTheme()` write-through that powers
 * `theme-switcher.tsx`. The standalone `<ThemeSwitcher/>` carries its
 * own dropdown trigger, which cannot nest inside a Radix menu — the
 * submenu re-binds the same three `ThemePreference` values instead.
 */
import type { ReactElement } from "react";
import { Link } from "react-router";

import { logout } from "@/api/auth";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useAuthMe } from "@/hooks/useAuthMe";
import { useTheme } from "@/lib/theme/ThemeProvider";
import type { ThemePreference } from "@/lib/theme/cookie";

const THEME_OPTIONS: { value: ThemePreference; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/** Rail-bottom user chip; opens the account dropdown. */
export function UserChip(): ReactElement | null {
  const { data: me } = useAuthMe();
  const { preference, setPreference } = useTheme();
  if (me === undefined) return null;

  function handleSignOut(): void {
    void (async () => {
      try {
        await logout();
      } catch (error) {
        // Navigate regardless — the login surface is the safe landing
        // either way; keep the operator-actionable detail in console.
        console.error("[UserMenu] logout request failed", error);
      }
      window.location.assign("/login");
    })();
  }

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button
          type="button"
          className="hover:bg-surface focus-visible:ring-accent flex w-full items-center gap-2.5 rounded-md p-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2"
        >
          <span
            aria-hidden="true"
            className="bg-surface-2 text-fg-muted border-border flex size-7 shrink-0 items-center justify-center rounded-full border font-mono text-[0.65rem] tracking-wide"
          >
            {initials(me.display_name)}
          </span>
          <span className="text-fg min-w-0 truncate text-sm">{me.display_name}</span>
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent side="top" align="start" className="w-56">
        <DropdownMenuLabel>
          <span className="block">{me.display_name}</span>
          {me.email !== null ? (
            <span className="text-fg-faint block text-xs font-normal">{me.email}</span>
          ) : null}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled>
          Settings
          <span className="text-fg-faint ml-auto text-xs">planned</span>
        </DropdownMenuItem>
        <DropdownMenuItem asChild>
          <Link to="/account/password">Change password</Link>
        </DropdownMenuItem>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>Theme</DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup
              value={preference}
              onValueChange={(value) => {
                // Narrow through the closed options list — the radio
                // group's payload type is `string` (no `as` casts per
                // frontend/CLAUDE.md).
                const opt = THEME_OPTIONS.find((o) => o.value === value);
                if (opt) void setPreference(opt.value);
              }}
            >
              {THEME_OPTIONS.map((opt) => (
                <DropdownMenuRadioItem key={opt.value} value={opt.value}>
                  {opt.label}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSeparator />
        <DropdownMenuItem onSelect={handleSignOut}>Sign out</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** First letters of the first two words, uppercased ("Ada Lovelace" → "AL"). */
function initials(displayName: string): string {
  return displayName
    .split(/\s+/)
    .filter((word) => word.length > 0)
    .slice(0, 2)
    .map((word) => word.charAt(0).toUpperCase())
    .join("");
}
