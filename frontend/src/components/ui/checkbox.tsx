import type * as React from "react";
import { CheckIcon } from "lucide-react";
import {
  CheckboxField as AriaCheckboxField,
  CheckboxButton as AriaCheckboxButton,
  composeRenderProps,
} from "react-aria-components";

import { cn } from "@/lib/utils";

function Checkbox({
  className,
  children,
  ...props
}: React.ComponentProps<typeof AriaCheckboxField>): React.ReactElement {
  return (
    <AriaCheckboxField {...props} className="contents">
      {composeRenderProps(children, (children) => (
        <AriaCheckboxButton
          data-slot="checkbox"
          className={composeRenderProps(className, (className) => cn(
            "group/checkbox peer relative flex w-fit items-center gap-2 text-sm font-medium data-disabled:cursor-not-allowed data-disabled:opacity-50",
            className,
          ))}
        >
          <span
            data-slot="checkbox-indicator"
            aria-hidden="true"
            className="relative flex size-4 shrink-0 items-center justify-center rounded-[4px] border border-input transition-colors outline-none after:absolute after:-inset-x-3 after:-inset-y-2 group-data-focus-visible/checkbox:border-ring group-data-focus-visible/checkbox:ring-3 group-data-focus-visible/checkbox:ring-ring/50 group-data-invalid/checkbox:border-destructive group-data-invalid/checkbox:ring-3 group-data-invalid/checkbox:ring-destructive/20 dark:bg-input/30 dark:group-data-invalid/checkbox:border-destructive/50 dark:group-data-invalid/checkbox:ring-destructive/40 group-data-selected/checkbox:border-primary group-data-selected/checkbox:bg-primary group-data-selected/checkbox:text-primary-foreground dark:group-data-selected/checkbox:bg-primary"
          >
            <CheckIcon className="size-3.5 opacity-0 group-data-selected/checkbox:opacity-100" />
          </span>
          {children}
        </AriaCheckboxButton>
      ))}
    </AriaCheckboxField>
  );
}

export { Checkbox };
export { CheckboxGroup } from "react-aria-components";
