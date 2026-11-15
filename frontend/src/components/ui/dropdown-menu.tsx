import type { ComponentProps, ReactElement, ReactNode } from "react";
import { Menu, MenuItem, Popover, Separator } from "react-aria-components";
import { cn } from "@/lib/utils";

type ContentProps = Omit<ComponentProps<typeof Popover>, "children" | "className"> & {
  children: ReactNode;
  className?: string;
};

function DropdownMenuContent({ className, children, ...props }: ContentProps): ReactElement {
  return (
    <Popover
      className={cn(
        "z-50 min-w-32 rounded-lg bg-popover p-1 text-popover-foreground border border-hairline shadow-raised",
        className,
      )}
      {...props}
    >
      <Menu className="outline-none">{children}</Menu>
    </Popover>
  );
}

function DropdownMenuItem({
  className,
  children,
  textValue,
  ...props
}: ComponentProps<typeof MenuItem>): ReactElement {
  return (
    <MenuItem
      data-slot="dropdown-menu-item"
      textValue={textValue ?? (typeof children === "string" ? children : undefined)}
      className={cn(
        "flex cursor-default items-center gap-2 rounded-[9px] px-2 py-1.5 text-sm outline-none data-focused:bg-surface-2 data-hovered:bg-surface-2 data-pressed:bg-surface-3 data-selected:font-semibold data-selected:text-accent-text data-disabled:opacity-50",
        typeof className === "string" ? className : undefined,
      )}
      {...props}
    >
      {children}
    </MenuItem>
  );
}

function DropdownMenuSeparator(props: ComponentProps<typeof Separator>): ReactElement {
  return <Separator className="-mx-1 my-1 h-px bg-border" {...props} />;
}

export {
  MenuTrigger as DropdownMenu,
  MenuSection as DropdownMenuGroup,
  MenuSection as DropdownMenuRadioGroup,
  Header as DropdownMenuLabel,
  SubmenuTrigger as DropdownMenuSub,
} from "react-aria-components";
export {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuItem as DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuItem as DropdownMenuSubTrigger,
  DropdownMenuContent as DropdownMenuSubContent,
};
