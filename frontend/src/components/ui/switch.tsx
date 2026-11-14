import type * as React from "react";
import {
  SwitchField as AriaSwitchField,
  SwitchButton as AriaSwitchButton,
  composeRenderProps,
} from "react-aria-components";

import { cn } from "@/lib/utils";

function SwitchField(props: React.ComponentProps<typeof AriaSwitchField>): React.ReactElement {
  return <AriaSwitchField {...props} className="contents" />;
}

function SwitchButton({
  className,
  size = "default",
  children,
  ...props
}: React.ComponentProps<typeof AriaSwitchButton> & {
  size?: "sm" | "default";
}): React.ReactElement {
  return (
    <AriaSwitchButton
      {...props}
      data-slot="switch"
      data-size={size}
      className={composeRenderProps(className, (className) => cn(
        "peer group/switch relative inline-flex shrink-0 items-center rounded-full border-[1.5px] border-border-control bg-border-control dark:bg-surface transition-colors after:absolute after:-inset-x-3 after:-inset-y-2 data-hovered:bg-border-control dark:data-hovered:bg-surface-2 data-pressed:bg-border-control dark:data-pressed:bg-surface-3 data-focus-visible:outline-[1.5px] data-focus-visible:outline-(--focus-ring) data-focus-visible:shadow-[0_0_0_5px_var(--focus-halo)] data-invalid:border-danger data-invalid:text-danger-text data-[size=default]:h-5 data-[size=default]:w-9 data-[size=sm]:h-4 data-[size=sm]:w-7 data-selected:bg-accent data-selected:data-hovered:bg-accent-strong data-selected:data-pressed:bg-accent-strong data-selected:border-accent-text data-disabled:cursor-not-allowed data-disabled:opacity-50",
        className,
      ))}
    >
      {composeRenderProps(children, (children) => (
        <>
          <span
            data-slot="switch-thumb"
            aria-hidden="true"
            className="pointer-events-none block translate-x-px rounded-full border border-border-control bg-sheet dark:border-transparent dark:bg-fg transition-transform motion-reduce:transition-none group-data-[size=default]/switch:size-3.5 group-data-[size=sm]/switch:size-2.5 group-data-selected/switch:translate-x-[17px] group-data-[size=sm]/switch:group-data-selected/switch:translate-x-[13px] group-data-selected/switch:border-transparent group-data-selected/switch:bg-fg-on-accent"
          />
          {children}
        </>
      ))}
    </AriaSwitchButton>
  );
}

function Switch({
  className,
  size = "default",
  children,
  ...props
}: React.ComponentProps<typeof AriaSwitchField> & { size?: "sm" | "default" }): React.ReactElement {
  return (
    <SwitchField {...props}>
      {composeRenderProps(children, (children) => (
        <SwitchButton size={size} className={className}>
          {children}
        </SwitchButton>
      ))}
    </SwitchField>
  );
}

export { Switch, SwitchButton, SwitchField };
