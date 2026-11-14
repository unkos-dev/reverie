import type * as React from "react"

import { cn } from "@/lib/utils"

function Textarea({ className, ...props }: React.ComponentProps<"textarea">) {
  return (
    <textarea
      data-slot="textarea"
      className={cn(
        "field-sizing-content min-h-16 w-full rounded-[9px] border border-border-control bg-surface px-2.5 py-1 text-sm text-fg transition-colors placeholder:text-fg-faint enabled:hover:bg-surface-2 enabled:active:bg-surface-3 disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-danger aria-invalid:text-danger-text",
        className
      )}
      {...props}
    />
  )
}

export { Textarea }
