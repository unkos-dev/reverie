import type * as React from "react"

import { cn } from "@/lib/utils"

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="input"
      className={cn(
        "h-[34px] min-w-0 file:border-0 file:bg-transparent file:text-fg w-full rounded-[9px] border border-border-control bg-transparent dark:bg-surface px-2.5 py-1 text-sm text-fg transition-colors placeholder:text-fg-faint enabled:hover:bg-surface dark:enabled:hover:bg-surface-2 enabled:active:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-danger aria-invalid:text-danger-text",
        className
      )}
      {...props}
    />
  )
}

export { Input }
