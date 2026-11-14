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
          className={composeRenderProps(className, (className) => cn("group/radio peer flex w-fit items-center gap-2 text-sm font-normal data-invalid:text-danger-text data-disabled:cursor-not-allowed data-disabled:opacity-50", className))}
        >
          <span aria-hidden="true" data-slot="radio-group-indicator" className="relative flex size-[17px] shrink-0 items-center justify-center rounded-full border-[1.5px] border-border-control bg-surface transition-colors after:absolute after:-inset-x-3 after:-inset-y-2 group-data-hovered/radio:bg-surface-2 group-data-pressed/radio:bg-surface-3 group-data-selected/radio:border-accent-text group-data-invalid/radio:border-danger group-data-focus-visible/radio:outline-[1.5px] group-data-focus-visible/radio:outline-(--focus-ring) group-data-focus-visible/radio:shadow-[0_0_0_5px_var(--focus-halo)]">
            <span className="size-2 rounded-full bg-accent-text opacity-0 group-data-selected/radio:opacity-100" />
          </span>
          {children}
        </AriaRadioButton>
      ))}
    </AriaRadioField>
  );
}

export { RadioGroup, Radio };
