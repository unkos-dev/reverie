import type * as React from "react";
import {
  SwitchField as AriaSwitchField,
  SwitchButton as AriaSwitchButton,
  composeRenderProps,
} from "react-aria-components";

import { cn } from "@/lib/utils";

function Switch({
  className,
  size = "default",
  children,
  ...props
}: React.ComponentProps<typeof AriaSwitchField> & { size?: "sm" | "default" }): React.ReactElement {
  return (
    <AriaSwitchField {...props} className="contents">
      {composeRenderProps(children, (children) => (
        <AriaSwitchButton
          data-slot="switch"
          data-size={size}
          className={composeRenderProps(className, (className) => cn(
            "peer group/switch relative inline-flex shrink-0 items-center rounded-full border border-transparent bg-input transition-all outline-none after:absolute after:-inset-x-3 after:-inset-y-2 data-focus-visible:border-ring data-focus-visible:ring-3 data-focus-visible:ring-ring/50 data-invalid:border-destructive data-invalid:ring-3 data-invalid:ring-destructive/20 dark:data-invalid:border-destructive/50 dark:data-invalid:ring-destructive/40 data-[size=default]:h-[18.4px] data-[size=default]:w-[32px] data-[size=sm]:h-[14px] data-[size=sm]:w-[24px] data-selected:bg-primary dark:bg-input/80 dark:data-selected:bg-primary data-disabled:cursor-not-allowed data-disabled:opacity-50",
            className,
          ))}
        >
          <span
            data-slot="switch-thumb"
            aria-hidden="true"
            className="pointer-events-none block translate-x-0 rounded-full bg-background ring-0 transition-transform group-data-[size=default]/switch:size-4 group-data-[size=sm]/switch:size-3 group-data-selected/switch:translate-x-[calc(100%-2px)] dark:bg-foreground dark:group-data-selected/switch:bg-primary-foreground"
          />
          {children}
        </AriaSwitchButton>
      ))}
    </AriaSwitchField>
  );
}

export { Switch };
