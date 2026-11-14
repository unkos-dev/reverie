import type { ComponentProps, ReactElement, ReactNode } from "react";
import { Button, ListBox, ListBoxItem, Popover } from "react-aria-components";
import { CheckIcon, ChevronDownIcon } from "lucide-react";

import { cn } from "@/lib/utils";

function SelectTrigger({
  className,
  children,
  size = "default",
  ...props
}: Omit<ComponentProps<typeof Button>, "children"> & {
  children?: ReactNode;
  size?: "sm" | "default";
}): ReactElement {
  return (
    <Button
      data-slot="select-trigger"
      data-size={size}
      className={cn(
        "flex w-fit items-center justify-between gap-1.5 rounded-[9px] border border-border-control bg-transparent dark:bg-surface py-1 pr-2 pl-2.5 text-sm text-fg whitespace-nowrap transition-colors data-hovered:bg-surface dark:data-hovered:bg-surface-2 data-pressed:bg-surface-3 data-disabled:opacity-50 in-[[data-invalid]]:border-danger data-invalid:border-danger data-invalid:text-danger-text data-[size=default]:h-[34px] data-[size=sm]:h-7",
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    >
      {children}
      <ChevronDownIcon className="size-4 text-muted-foreground" />
    </Button>
  );
}

function SelectContent({
  className,
  children,
  ...props
}: Omit<ComponentProps<typeof Popover>, "children"> & { children: ReactNode }): ReactElement {
  return (
    <Popover
      data-slot="select-content"
      className={cn(
        "z-50 min-w-(--trigger-width) overflow-auto rounded-lg bg-popover p-1 text-popover-foreground border border-hairline shadow-raised",
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    >
      <ListBox className="max-h-80 overflow-auto outline-none">{children}</ListBox>
    </Popover>
  );
}

function SelectItem({
  className,
  children,
  textValue,
  ...props
}: ComponentProps<typeof ListBoxItem>): ReactElement {
  return (
    <ListBoxItem
      textValue={textValue ?? (typeof children === "string" ? children : undefined)}
      data-slot="select-item"
      className={cn(
        "flex cursor-default items-center gap-2 rounded-[9px] px-2 py-1.5 text-sm outline-none data-focused:bg-surface-2 data-hovered:bg-surface-2 data-pressed:bg-surface-3 data-disabled:opacity-50",
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    >
      {(state) => (
        <>
          <span className="flex-1">
            {typeof children === "function" ? children(state) : children}
          </span>
          {state.isSelected && <CheckIcon className="size-4" />}
        </>
      )}
    </ListBoxItem>
  );
}

export { Select, SelectValue } from "react-aria-components";
export { SelectContent, SelectItem, SelectTrigger };
