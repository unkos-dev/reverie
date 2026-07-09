import type { ReactElement } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/lib/theme/ThemeProvider";
import type { ThemePreference } from "@/lib/theme/cookie";

const OPTIONS: { value: ThemePreference; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

/**
 * App-shell theme switcher. Renders a shadcn `DropdownMenu` with a radio
 * group of the three `ThemePreference` values plus a status line that
 * surfaces what the user picked (or `"auto"` when they deferred to the
 * OS). Reads + writes the theme stack via `useTheme()` — the provider
 * owns the optimistic-update + PATCH + rollback flow.
 *
 * The `onValueChange` handler narrows the radio group's `string` payload
 * through the closed `OPTIONS` list instead of an `as ThemePreference`
 * cast (the radio group's value type is `string`). An unknown
 * value is silently ignored — the radio group cannot emit one in
 * practice, but defending against shadcn API drift here costs one
 * `.find` call.
 *
 * @returns The dropdown subtree. Must be mounted inside a
 *   `<ThemeProvider>` — `useTheme()` throws otherwise.
 */
export function ThemeSwitcher(): ReactElement {
  const { preference, setPreference } = useTheme();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="sm" aria-label="Theme preference">
          Theme: {preference}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent>
        <DropdownMenuRadioGroup
          value={preference}
          onValueChange={(value) => {
            // Narrow via OPTIONS (the closed source of values the radio
            // group can emit) instead of an `as` assertion.
            const opt = OPTIONS.find((o) => o.value === value);
            if (opt) void setPreference(opt.value);
          }}
        >
          {OPTIONS.map((opt) => (
            <DropdownMenuRadioItem key={opt.value} value={opt.value}>
              {opt.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuItem disabled className="text-fg-faint">
          {preference === "system" ? "auto" : `set: ${preference}`}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
