import type { ComponentProps, KeyboardEventHandler, ReactElement, ReactNode } from "react";
import {
  Button,
  Dialog as AriaDialog,
  Heading,
  Modal,
  ModalOverlay,
  Text,
} from "react-aria-components";
import { XIcon } from "lucide-react";

import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";

type DialogContentProps = Omit<ComponentProps<typeof AriaDialog>, "children"> & {
  children: ReactNode;
  showCloseButton?: boolean;
  isDismissable?: boolean;
  isOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  onKeyDown?: KeyboardEventHandler<HTMLElement>;
};

function DialogContent({
  className,
  children,
  showCloseButton = true,
  isDismissable = true,
  isOpen,
  onOpenChange,
  onKeyDown,
  ...props
}: DialogContentProps): ReactElement {
  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      isDismissable={isDismissable}
      className="fixed inset-0 z-50 bg-overlay supports-backdrop-filter:backdrop-blur-xs"
    >
      <Modal className="contents">
        <AriaDialog
          render={(dialogProps) => <section {...dialogProps} onKeyDown={onKeyDown} />}
          data-slot="dialog-content"
          className={cn(
            "fixed top-1/2 left-1/2 z-50 grid w-full max-w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2 gap-4 rounded-xl bg-popover p-4 text-sm text-popover-foreground ring-1 ring-foreground/10 outline-none sm:max-w-sm",
            className,
          )}
          {...props}
        >
          {children}
          {showCloseButton && (
            <Button
              slot="close"
              aria-label="Close"
              className={cn(
                buttonVariants({ variant: "ghost", size: "icon-sm" }),
                "absolute top-2 right-2",
              )}
            >
              <XIcon />
            </Button>
          )}
        </AriaDialog>
      </Modal>
    </ModalOverlay>
  );
}

function DialogClose({ className, ...props }: ComponentProps<typeof Button>): ReactElement {
  return (
    <Button
      slot="close"
      className={cn(
        buttonVariants({ variant: "outline" }),
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    />
  );
}

function DialogHeader({ className, ...props }: ComponentProps<"div">): ReactElement {
  return (
    <div data-slot="dialog-header" className={cn("flex flex-col gap-2", className)} {...props} />
  );
}

function DialogFooter({ className, ...props }: ComponentProps<"div">): ReactElement {
  return (
    <div
      data-slot="dialog-footer"
      className={cn(
        "-mx-4 -mb-4 flex flex-col-reverse gap-2 rounded-b-xl border-t bg-muted/50 p-4 sm:flex-row sm:justify-end",
        className,
      )}
      {...props}
    />
  );
}

function DialogTitle({ className, ...props }: ComponentProps<typeof Heading>): ReactElement {
  return (
    <Heading
      slot="title"
      data-slot="dialog-title"
      className={cn("text-base leading-none font-medium", className)}
      {...props}
    />
  );
}

function DialogDescription({ className, ...props }: ComponentProps<typeof Text>): ReactElement {
  return (
    <Text
      slot="description"
      data-slot="dialog-description"
      className={cn("text-sm text-muted-foreground", className)}
      {...props}
    />
  );
}

export { DialogTrigger as Dialog } from "react-aria-components";
export {
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
};
