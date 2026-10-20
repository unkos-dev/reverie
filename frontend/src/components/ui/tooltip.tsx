import type { ComponentProps, ReactElement } from "react";
import { Tooltip as AriaTooltip } from "react-aria-components";
import { cn } from "@/lib/utils";

function TooltipContent({ className, ...props }: ComponentProps<typeof AriaTooltip>): ReactElement {
  return (
    <AriaTooltip
      data-slot="tooltip-content"
      className={cn(
        "z-50 max-w-xs rounded-md bg-foreground px-3 py-1.5 text-xs text-background",
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    />
  );
}

export { TooltipTrigger as Tooltip, Button as TooltipTrigger } from "react-aria-components";
export { TooltipContent };
