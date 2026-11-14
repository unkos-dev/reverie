import type { ComponentProps, ReactElement } from "react";
import {
  ToggleButtonGroup as AriaToggleButtonGroup,
  ToggleButton as AriaToggleButton,
  composeRenderProps,
} from "react-aria-components";

import { cn } from "@/lib/utils";

function ToggleButtonGroup({ className, ...props }: ComponentProps<typeof AriaToggleButtonGroup>): ReactElement {
  return (
    <AriaToggleButtonGroup
      data-slot="toggle-button-group"
      className={composeRenderProps(className, (className) => cn(
        "inline-flex h-[34px] w-fit items-center rounded-[10px] border border-hairline bg-surface p-[2px] data-[orientation=vertical]:h-auto data-[orientation=vertical]:flex-col",
        className,
      ))}
      {...props}
    />
  );
}

function ToggleButton({ className, ...props }: ComponentProps<typeof AriaToggleButton>): ReactElement {
  return (
    <AriaToggleButton
      data-slot="toggle-button"
      className={composeRenderProps(className, (className) => cn(
        "inline-flex h-7 items-center justify-center gap-[7px] rounded-[7px] border border-transparent px-[11px] text-sm font-medium text-fg-muted transition-colors data-hovered:bg-surface-2 data-pressed:bg-surface-3 data-selected:border-accent-text data-selected:bg-surface-2 data-selected:text-fg data-disabled:pointer-events-none data-disabled:opacity-50 [&_svg]:size-4",
        className,
      ))}
      {...props}
    />
  );
}

export { ToggleButtonGroup, ToggleButton };
