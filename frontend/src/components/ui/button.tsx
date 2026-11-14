import type * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"
import { Button as AriaButton, composeRenderProps } from "react-aria-components"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "group/button inline-flex shrink-0 items-center justify-center rounded-[9px] border border-hairline bg-clip-padding text-sm font-medium whitespace-nowrap transition-colors select-none data-disabled:pointer-events-none data-disabled:opacity-50 data-pending:opacity-50 aria-invalid:border-danger aria-invalid:text-danger-text [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-accent text-fg-on-accent data-hovered:bg-accent-strong data-pressed:bg-accent-strong [a]:hover:bg-accent-strong [a]:active:bg-accent-strong",
        outline: "bg-sheet text-fg data-hovered:bg-surface data-pressed:bg-surface-2 [a]:hover:bg-surface [a]:active:bg-surface-2 aria-expanded:bg-surface",
        secondary: "bg-surface text-fg data-hovered:bg-surface-2 data-pressed:bg-surface-3 [a]:hover:bg-surface-2 [a]:active:bg-surface-3",
        ghost: "border-transparent bg-transparent text-fg-muted data-hovered:bg-surface data-pressed:bg-surface-2 [a]:hover:bg-surface [a]:active:bg-surface-2 aria-expanded:bg-surface",
        destructive: "bg-danger-soft text-danger-text data-hovered:bg-surface-2 data-pressed:bg-surface-3 [a]:hover:bg-surface-2 [a]:active:bg-surface-3",
        link: "border-transparent text-accent-text underline-offset-4 [a]:hover:underline data-hovered:underline",
      },
      size: {
        default:
          "h-[34px] gap-[7px] px-3 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        xs: "h-6 gap-1 rounded-[min(var(--radius-md),10px)] px-2 text-xs in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3",
        sm: "h-7 gap-1 rounded-[min(var(--radius-md),12px)] px-2.5 text-[0.8rem] in-data-[slot=button-group]:rounded-lg has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 [&_svg:not([class*='size-'])]:size-3.5",
        lg: "h-9 gap-1.5 px-2.5 has-data-[icon=inline-end]:pr-2 has-data-[icon=inline-start]:pl-2",
        icon: "size-[34px]",
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
