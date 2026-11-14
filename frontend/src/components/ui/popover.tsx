import type { ComponentProps, ReactElement, ReactNode } from "react";
import { Dialog, Popover as AriaPopover } from "react-aria-components";
import { cn } from "@/lib/utils";

type PopoverContentProps = Omit<ComponentProps<typeof AriaPopover>, "children" | "className"> & {
  children: ReactNode;
  className?: string;
};

function PopoverContent({
  className,
  children,
  "aria-label": label,
  ...props
}: PopoverContentProps): ReactElement {
  return (
    <AriaPopover
      data-slot="popover-content"
      className={cn(
        "z-50 w-72 rounded-lg bg-popover p-2.5 text-sm text-popover-foreground border border-hairline shadow-raised outline-none",
        className,
      )}
      {...props}
    >
      <Dialog aria-label={label} className="outline-none">
        {children}
      </Dialog>
    </AriaPopover>
  );
}

export { DialogTrigger as Popover } from "react-aria-components";
export { PopoverContent };
