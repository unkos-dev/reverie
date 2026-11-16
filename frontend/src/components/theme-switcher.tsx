import type { ReactElement } from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/lib/theme/ThemeProvider";
import type { ThemePreference } from "@/lib/theme/cookie";

const OPTIONS: { value: ThemePreference | "monochrome"; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "monochrome", label: "Monochrome" },
];

/** Choose an account theme and browser palette through the native menu. */
export function ThemeSwitcher(): ReactElement {
  const { preference, palette, setPreference } = useTheme();
  return (
    <DropdownMenu>
      <Button variant="outline" size="sm" aria-label="Theme preference">
        Theme: {palette === "monochrome" && preference === "dark" ? "monochrome" : preference}
      </Button>
      <DropdownMenuContent>
        <DropdownMenuRadioGroup
          selectionMode="single"
          selectedKeys={[
            palette === "monochrome" && preference === "dark" ? "monochrome" : preference,
          ]}
          onSelectionChange={(keys) => {
            if (keys === "all") return;
            const value = keys.values().next().value;
            const opt = OPTIONS.find((o) => o.value === value);
            if (opt) {
              void setPreference(
                opt.value === "monochrome" ? "dark" : opt.value,
                opt.value === "monochrome" ? "monochrome" : "periwinkle",
              );
            }
          }}
        >
          {OPTIONS.map((opt) => (
            <DropdownMenuRadioItem key={opt.value} id={opt.value}>
              {opt.label}
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        <DropdownMenuItem isDisabled className="text-fg-faint">
          {preference === "system" ? "auto" : `set: ${preference}`}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
