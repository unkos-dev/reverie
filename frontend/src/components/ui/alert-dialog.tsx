import type { ComponentProps, ReactElement } from "react";
import { Button } from "react-aria-components";
import { type Button as StyledButton, buttonVariants } from "./button";
import { DialogContent } from "./dialog";
import { cn } from "@/lib/utils";

type ActionProps = ComponentProps<typeof Button> &
  Pick<ComponentProps<typeof StyledButton>, "variant" | "size">;

function AlertDialogContent({
  className,
  ...props
}: ComponentProps<typeof DialogContent>): ReactElement {
  return (
    <DialogContent
      role="alertdialog"
      isDismissable={false}
      showCloseButton={false}
      className={cn("max-w-xs sm:max-w-sm", className)}
      {...props}
    />
  );
}

function AlertDialogAction({
  variant = "default",
  size = "default",
  className,
  ...props
}: ActionProps): ReactElement {
  return (
    <Button
      data-slot="alert-dialog-action"
      className={cn(
        buttonVariants({ variant, size }),
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    />
  );
}

function AlertDialogCancel({
  variant = "outline",
  size = "default",
  className,
  ...props
}: ActionProps): ReactElement {
  return (
    <Button
      slot="close"
      autoFocus
      data-slot="alert-dialog-cancel"
      className={cn(
        buttonVariants({ variant, size }),
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    />
  );
}

export {
  Dialog as AlertDialog,
  DialogTrigger as AlertDialogTrigger,
  DialogHeader as AlertDialogHeader,
  DialogFooter as AlertDialogFooter,
  DialogTitle as AlertDialogTitle,
  DialogDescription as AlertDialogDescription,
} from "./dialog";
export { AlertDialogContent, AlertDialogAction, AlertDialogCancel };
