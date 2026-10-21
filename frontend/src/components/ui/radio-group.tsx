import type * as React from "react";
import {
  RadioGroup as AriaRadioGroup,
  RadioField as AriaRadioField,
  RadioButton as AriaRadioButton,
  composeRenderProps,
} from "react-aria-components";

import { cn } from "@/lib/utils";

function RadioGroup({
  className,
  ...props
}: React.ComponentProps<typeof AriaRadioGroup>): React.ReactElement {
  return (
    <AriaRadioGroup
      data-slot="radio-group"
      className={composeRenderProps(className, (className) => cn("grid w-full gap-2", className))}
      {...props}
    />
  );
}

function Radio({
  className,
  children,
  ...props
}: React.ComponentProps<typeof AriaRadioField>): React.ReactElement {
  return (
    <AriaRadioField {...props} className="contents">
      {composeRenderProps(children, (children) => (
        <AriaRadioButton
          data-slot="radio-group-item"
          className={composeRenderProps(className, (className) => cn("group/radio peer flex w-fit items-center gap-2 text-sm font-normal data-disabled:cursor-not-allowed data-disabled:opacity-50", className))}
        >
          <span aria-hidden="true" data-slot="radio-group-indicator" className="relative flex size-4 shrink-0 items-center justify-center rounded-full border border-input outline-none after:absolute after:-inset-x-3 after:-inset-y-2 group-data-focus-visible/radio:border-ring group-data-focus-visible/radio:ring-3 group-data-focus-visible/radio:ring-ring/50 group-data-invalid/radio:border-destructive group-data-invalid/radio:ring-3 group-data-invalid/radio:ring-destructive/20 dark:bg-input/30 dark:group-data-invalid/radio:border-destructive/50 dark:group-data-invalid/radio:ring-destructive/40 group-data-selected/radio:border-primary group-data-selected/radio:bg-primary dark:group-data-selected/radio:bg-primary">
            <span className="size-2 rounded-full bg-primary-foreground opacity-0 group-data-selected/radio:opacity-100" />
          </span>
          {children}
        </AriaRadioButton>
      ))}
    </AriaRadioField>
  );
}

export { RadioGroup, Radio };
