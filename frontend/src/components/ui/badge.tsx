import type * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "group/badge inline-flex h-[30px] w-fit shrink-0 items-center justify-center gap-[7px] rounded-[8px] border border-hairline px-2.5 text-xs font-medium whitespace-nowrap transition-colors aria-invalid:border-danger aria-invalid:text-danger-text [&>svg]:pointer-events-none [&>svg]:size-3!",
  {
    variants: {
      variant: {
        default: "bg-surface text-fg",
        secondary: "bg-surface-2 text-fg",
        destructive:
          "bg-danger-soft text-danger-text",
        outline: "bg-transparent text-fg",
        ghost: "bg-transparent text-fg-muted",
        link: "text-accent-text underline-offset-4",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

function Badge({
  className,
  variant = "default",
  ...props
}: React.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return (
    <span
      data-slot="badge"
      data-variant={variant}
      className={cn(badgeVariants({ variant }), className)}
      {...props}
    />
  );
}

// oxlint-disable-next-line react/only-export-components -- shadcn pattern: cva const + component co-located.
export { Badge, badgeVariants };
