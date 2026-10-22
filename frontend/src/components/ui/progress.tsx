import type * as React from "react"
import { ProgressBar, composeRenderProps } from "react-aria-components"

import { cn } from "@/lib/utils"

function Progress({
  className,
  ...props
}: Omit<React.ComponentProps<typeof ProgressBar>, "children">): React.ReactElement {
  return (
    <ProgressBar
      data-slot="progress"
      className={composeRenderProps(className, (className) =>
        cn(
          "relative flex h-1 w-full items-center overflow-x-hidden rounded-full bg-muted",
          className
        )
      )}
      {...props}
    >
      {({ percentage }) => (
        <div
          data-slot="progress-indicator"
          className="size-full flex-1 bg-primary transition-all"
          style={{ transform: `translateX(-${String(100 - (percentage ?? 0))}%)` }}
        />
      )}
    </ProgressBar>
  )
}

export { Progress }
