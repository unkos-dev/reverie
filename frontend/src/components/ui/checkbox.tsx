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
            "group/checkbox peer relative flex w-fit items-center gap-2 text-sm font-medium data-invalid:text-danger-text data-disabled:cursor-not-allowed data-disabled:opacity-50",
            className,
          ))}
        >
          <span
            data-slot="checkbox-indicator"
            aria-hidden="true"
            className="relative flex size-[17px] shrink-0 items-center justify-center rounded-[5.5px] border-[1.5px] border-border-control bg-surface transition-colors after:absolute after:-inset-x-3 after:-inset-y-2 group-data-hovered/checkbox:bg-surface-2 group-data-pressed/checkbox:bg-surface-3 group-data-selected/checkbox:border-accent-text group-data-selected/checkbox:bg-accent group-data-selected/checkbox:group-data-hovered/checkbox:bg-accent-strong group-data-selected/checkbox:group-data-pressed/checkbox:bg-accent-strong group-data-selected/checkbox:text-fg-on-accent group-data-indeterminate/checkbox:border-accent-text group-data-indeterminate/checkbox:bg-accent group-data-indeterminate/checkbox:group-data-hovered/checkbox:bg-accent-strong group-data-indeterminate/checkbox:group-data-pressed/checkbox:bg-accent-strong group-data-indeterminate/checkbox:after:inset-auto group-data-indeterminate/checkbox:after:h-[1.5px] group-data-indeterminate/checkbox:after:w-2 group-data-indeterminate/checkbox:after:bg-fg-on-accent group-data-invalid/checkbox:border-danger"
          >
            <CheckIcon className="size-3.5 opacity-0 group-data-selected/checkbox:opacity-100 group-data-indeterminate/checkbox:hidden" />
          </span>
          {children}
        </AriaCheckboxButton>
      ))}
    </AriaCheckboxField>
  );
}

export { Checkbox };
export { CheckboxGroup } from "react-aria-components";
