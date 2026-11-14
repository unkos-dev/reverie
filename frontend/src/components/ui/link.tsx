import type { ReactElement } from "react";
import { Link as AriaLink, composeRenderProps, type LinkProps } from "react-aria-components";

import { cn } from "@/lib/utils";

export function Link({ className, ...props }: LinkProps): ReactElement {
  return (
    <AriaLink
      {...props}
      className={composeRenderProps(className, (value) => cn(
        "inline-flex items-center justify-center gap-2 rounded-sm text-sm font-medium text-fg-muted outline-none transition-colors data-hovered:bg-surface-2 data-hovered:text-fg aria-[current=page]:bg-surface-2 aria-[current=page]:text-fg motion-reduce:transition-none",
        value,
      ))}
    />
  );
}
