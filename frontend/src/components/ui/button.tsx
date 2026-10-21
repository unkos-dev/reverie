import type * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Button as AriaButton, composeRenderProps } from "react-aria-components"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-lg border border-transparent bg-clip-padding text-sm font-medium whitespace-nowrap transition-all outline-none select-none data-focus-visible:border-ring data-focus-visible:ring-3 data-focus-visible:ring-ring/50 [a]:focus-visible:border-ring [a]:focus-visible:ring-3 [a]:focus-visible:ring-ring/50 data-pressed:not-aria-[haspopup]:translate-y-px [a]:active:not-aria-[haspopup]:translate-y-px data-disabled:pointer-events-none data-disabled:opacity-50 data-pending:opacity-50 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground data-hovered:bg-primary/80 [a]:hover:bg-primary/80",
        outline:
          "border-border bg-background [a]:hover:bg-muted data-hovered:bg-muted [a]:hover:text-foreground data-hovered:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:border-input dark:bg-input/30 dark:[a]:hover:bg-input/50 dark:data-hovered:bg-input/50",
        secondary:
          "bg-secondary text-secondary-foreground [a]:hover:bg-secondary/80 data-hovered:bg-secondary/80 aria-expanded:bg-secondary aria-expanded:text-secondary-foreground",
        ghost:
          "[a]:hover:bg-muted data-hovered:bg-muted [a]:hover:text-foreground data-hovered:text-foreground aria-expanded:bg-muted aria-expanded:text-foreground dark:[a]:hover:bg-muted/50 dark:data-hovered:bg-muted/50",
        destructive:
          "bg-destructive/10 text-destructive [a]:hover:bg-destructive/20 data-hovered:bg-destructive/20 data-focus-visible:border-destructive/40 data-focus-visible:ring-destructive/20 [a]:focus-visible:border-destructive/40 [a]:focus-visible:ring-destructive/20 dark:bg-destructive/20 dark:[a]:hover:bg-destructive/30 dark:data-hovered:bg-destructive/30 dark:data-focus-visible:ring-destructive/40 dark:[a]:focus-visible:ring-destructive/40",
        link: "text-primary underline-offset-4 [a]:hover:underline data-hovered:underline",
      },
      size: {
        default:
          "h-8 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        xs: "h-6 gap-1 rounded-[min(var(--radius-md),10px)] px-2 text-xs in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1 rounded-[min(var(--radius-md),12px)] px-2.5 text-[0.8rem] in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-9 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        icon: "size-8",
        "icon-xs":
          "size-6 rounded-[min(var(--radius-md),10px)] in-data-[slot=button-group]:rounded-lg [&_svg:not([class*='size-'])]:size-3",
        "icon-sm":
          "size-7 rounded-[min(var(--radius-md),12px)] in-data-[slot=button-group]:rounded-lg",
        "icon-lg": "size-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  ...props
}: React.ComponentProps<typeof AriaButton> & VariantProps<typeof buttonVariants>): React.ReactElement {
  return (
    <AriaButton
      data-slot="button"
      data-variant={variant}
      data-size={size}
      className={composeRenderProps(className, (className) =>
        cn(buttonVariants({ variant, size }), className)
      )}
      {...props}
    />
  )
}

// oxlint-disable-next-line react/only-export-components -- shadcn pattern: cva const + component co-located.
export { Button, buttonVariants }
