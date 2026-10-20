import type { ComponentProps, ReactElement } from "react";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from "./dialog";
import { cn } from "@/lib/utils";

function SheetContent({
  side = "right",
  className,
  ...props
}: ComponentProps<typeof DialogContent> & {
  side?: "top" | "right" | "bottom" | "left";
}): ReactElement {
  return (
    <DialogContent
      data-slot="sheet-content"
      data-side={side}
      className={cn(
        "top-0 left-auto flex h-full max-w-[100vw] translate-x-0 translate-y-0 flex-col rounded-none bg-clip-padding p-0 shadow-lg ring-0 data-[side=right]:right-0 data-[side=right]:w-3/4 data-[side=right]:border-l data-[side=left]:left-0 data-[side=left]:w-3/4 data-[side=left]:border-r data-[side=top]:left-0 data-[side=top]:h-auto data-[side=top]:max-w-none data-[side=top]:border-b data-[side=bottom]:top-auto data-[side=bottom]:bottom-0 data-[side=bottom]:left-0 data-[side=bottom]:h-auto data-[side=bottom]:max-w-none data-[side=bottom]:border-t data-[side=left]:sm:max-w-sm data-[side=right]:sm:max-w-sm",
        className,
      )}
      {...props}
    />
  );
}

function SheetHeader({ className, ...props }: ComponentProps<"div">): ReactElement {
  return (
    <div
      data-slot="sheet-header"
      className={cn("flex flex-col gap-0.5 p-4", className)}
      {...props}
    />
  );
}

function SheetFooter({ className, ...props }: ComponentProps<"div">): ReactElement {
  return (
    <div
      data-slot="sheet-footer"
      className={cn("mt-auto flex flex-col gap-2 p-4", className)}
      {...props}
    />
  );
}

function SheetTitle({ className, ...props }: ComponentProps<typeof DialogTitle>): ReactElement {
  return (
    <DialogTitle
      className={cn(
        "text-base font-medium leading-normal text-foreground",
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    />
  );
}

export {
  Dialog as Sheet,
  DialogTrigger as SheetTrigger,
  DialogClose as SheetClose,
  SheetContent,
  SheetHeader,
  SheetFooter,
  SheetTitle,
  DialogDescription as SheetDescription,
};
