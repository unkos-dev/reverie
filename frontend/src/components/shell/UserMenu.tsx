/**
 * Account menu in the floating top bar.
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
 * `theme-switcher.tsx`.
 */
import { Button } from "@/components/ui/button";
import type { ReactElement } from "react";

import { logout } from "@/api/auth";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuGroup,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from "@/components/ui/dropdown-menu";
import { useAuthMe } from "@/hooks/useAuthMe";
import { forgetActiveUser } from "@/lib/active-user";
import { useMediaQuery } from "@/lib/hooks/use-media-query";
import { useTheme } from "@/lib/theme/ThemeProvider";
import type { ThemePreference } from "@/lib/theme/cookie";

import { useShellIngestion } from "./use-shell-ingestion";

const THEME_OPTIONS: { value: ThemePreference | "monochrome"; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "monochrome", label: "Monochrome" },
];

/** Account avatar opening the account dropdown. */
export function UserChip(): ReactElement | null {
  const { data: me } = useAuthMe();
  const { preference, palette, failure, setPreference } = useTheme();
  const { attention } = useShellIngestion();
  const isDesktop = useMediaQuery("(min-width: 64rem)");
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
      // Whatever the request did, this browser no longer has a confirmed
      // account: per-user caches must stop resolving until the next
      // sign-in names one. The caches themselves stay for that return.
      forgetActiveUser();
      window.location.assign("/login");
    })();
  }

  return (
    <>
      <DropdownMenu>
        <Button
          type="button"
          variant="ghost"
          aria-label={`Account menu for ${me.display_name}`}
          className="data-hovered:bg-surface data-focus-visible:ring-accent flex size-9 items-center justify-center rounded-full p-0 text-left transition-colors data-focus-visible:outline-none data-focus-visible:ring-2"
        >
          <span
            aria-hidden="true"
            className="bg-surface-2 text-fg-muted border-border flex size-7 shrink-0 items-center justify-center rounded-full border font-mono text-[0.65rem] tracking-wide"
          >
            {initials(me.display_name)}
          </span>
        </Button>
        <DropdownMenuContent placement="bottom end" className="w-56">
          {me.role === "admin" && !isDesktop ? (
            <DropdownMenuGroup aria-label="Admin">
              <DropdownMenuItem href="/admin/users">Users</DropdownMenuItem>
              <DropdownMenuItem
                href="/admin/dashboard"
                textValue="Ingestion"
                aria-label={
                  attention > 0 ? `Ingestion, ${String(attention)} need attention` : "Ingestion"
                }
              >
                Ingestion
                {attention > 0 ? (
                  <span
                    aria-hidden="true"
                    className="bg-accent text-fg-on-accent ml-auto rounded-full px-1.5 text-xs tabular-nums"
                  >
                    {attention}
                  </span>
                ) : null}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
            </DropdownMenuGroup>
          ) : null}
          <DropdownMenuGroup>
            <DropdownMenuLabel>
              <span className="block">{me.display_name}</span>
              {me.email !== null ? (
                <span className="text-fg-faint block text-xs font-normal">{me.email}</span>
              ) : null}
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            {me.role === "admin" ? (
              <DropdownMenuItem href="/admin/settings">Settings</DropdownMenuItem>
            ) : null}
            <DropdownMenuItem href="/account/email">Email address</DropdownMenuItem>
            {me.has_local_password ? (
              <DropdownMenuItem href="/account/password">Change password</DropdownMenuItem>
            ) : null}
            <DropdownMenuItem href="/tokens">API tokens</DropdownMenuItem>
            <DropdownMenuSub>
              <DropdownMenuSubTrigger>Theme</DropdownMenuSubTrigger>
              <DropdownMenuSubContent>
                <DropdownMenuRadioGroup
                  selectionMode="single"
                  selectedKeys={[
                    palette === "monochrome" && preference === "dark" ? "monochrome" : preference,
                  ]}
                  onSelectionChange={(keys) => {
                    if (keys === "all") return;
                    const value = keys.values().next().value;
                    const opt = THEME_OPTIONS.find((o) => o.value === value);
                    if (opt) {
                      void setPreference(
                        opt.value === "monochrome" ? "dark" : opt.value,
                        opt.value === "monochrome" ? "monochrome" : "periwinkle",
                      );
                    }
                  }}
                >
                  {THEME_OPTIONS.map((opt) => (
                    <DropdownMenuRadioItem key={opt.value} id={opt.value}>
                      {opt.label}
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuSubContent>
            </DropdownMenuSub>
            <DropdownMenuSeparator />
            <DropdownMenuItem onAction={handleSignOut}>Sign out</DropdownMenuItem>
          </DropdownMenuGroup>
        </DropdownMenuContent>
      </DropdownMenu>
      {failure ? (
        <p role="alert" className="text-fg-muted px-2 pb-2 text-sm">
          {failure}
        </p>
      ) : null}
    </>
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
