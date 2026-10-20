import type { ComponentProps, ReactElement, ReactNode, Ref } from "react";
import { Dialog, Popover as AriaPopover } from "react-aria-components";
import { cn } from "@/lib/utils";

type PopoverContentProps = Omit<ComponentProps<typeof AriaPopover>, "children" | "className"> & {
  children: ReactNode;
  className?: string;
  dialogRef?: Ref<HTMLElement>;
};

function PopoverContent({
  className,
  children,
  dialogRef,
  "aria-label": label,
  ...props
}: PopoverContentProps): ReactElement {
  return (
    <AriaPopover
      data-slot="popover-content"
      className={cn(
        "z-50 w-72 rounded-lg bg-popover p-2.5 text-sm text-popover-foreground shadow-md ring-1 ring-foreground/10 outline-none",
        className,
      )}
      {...props}
    >
      <Dialog ref={dialogRef} aria-label={label} className="outline-none">
        {children}
      </Dialog>
    </AriaPopover>
  );
}

export { DialogTrigger as Popover, Pressable as PopoverTrigger } from "react-aria-components";
export { PopoverContent };
