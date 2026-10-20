import type { ComponentProps, ReactElement, ReactNode } from "react";
import { Button, ListBox, ListBoxItem, Popover, Select, SelectValue } from "react-aria-components";
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
        "flex w-fit items-center justify-between gap-1.5 rounded-lg border border-input bg-transparent py-2 pr-2 pl-2.5 text-sm whitespace-nowrap outline-none focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50 data-[size=default]:h-8 data-[size=sm]:h-7 dark:bg-input/30",
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
        "z-50 min-w-(--trigger-width) overflow-auto rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10",
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
        "flex cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-sm outline-none data-focused:bg-muted data-disabled:opacity-50",
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

export { Select, SelectContent, SelectItem, SelectTrigger, SelectValue };
