"use client";

import type * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import {
  Tabs as AriaTabs,
  TabList as AriaTabList,
  Tab as AriaTab,
  TabPanel as AriaTabPanel,
  composeRenderProps,
} from "react-aria-components";

import { cn } from "@/lib/utils";

function Tabs({
  className,
  orientation = "horizontal",
  ...props
}: React.ComponentProps<typeof AriaTabs>): React.ReactElement {
  return (
    <AriaTabs
      data-slot="tabs"
      orientation={orientation}
      className={composeRenderProps(className, (className) => cn("group/tabs flex gap-2 data-[orientation=horizontal]:flex-col", className))}
      {...props}
    />
  );
}

const tabsListVariants = cva(
  "group/tabs-list inline-flex w-fit items-center justify-center rounded-[10px] border border-hairline p-[3px] text-muted-foreground group-data-[orientation=horizontal]/tabs:h-[34px] group-data-[orientation=vertical]/tabs:h-fit group-data-[orientation=vertical]/tabs:flex-col data-[variant=line]:rounded-none",
  {
    variants: {
      variant: {
        default: "bg-surface",
        line: "gap-1 bg-transparent",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  },
);

function TabList({
  className,
  variant = "default",
  ...props
}: React.ComponentProps<typeof AriaTabList> & VariantProps<typeof tabsListVariants>): React.ReactElement {
  return (
    <AriaTabList
      data-slot="tabs-list"
      data-variant={variant}
      className={composeRenderProps(className, (className) => cn(tabsListVariants({ variant }), className))}
      {...props}
    />
  );
}

function Tab({ className, ...props }: React.ComponentProps<typeof AriaTab>): React.ReactElement {
  return (
    <AriaTab
      data-slot="tabs-trigger"
      className={composeRenderProps(className, (className) => cn(
        "relative inline-flex h-7 flex-1 items-center justify-center gap-[7px] rounded-[7px] border border-transparent px-[11px] text-sm font-medium whitespace-nowrap text-fg-muted transition-colors data-hovered:bg-surface-2 data-pressed:bg-surface-3 data-disabled:pointer-events-none data-disabled:opacity-50 group-data-[orientation=vertical]/tabs:w-full [&_svg]:size-4",
        "data-selected:border-accent-text data-selected:bg-surface-2 data-selected:text-fg group-data-[variant=line]/tabs-list:rounded-none group-data-[variant=line]/tabs-list:border-0 group-data-[variant=line]/tabs-list:border-b-2 group-data-[variant=line]/tabs-list:data-selected:border-accent-text",
        className,
      ))}
      {...props}
    />
  );
}

function TabPanel({ className, ...props }: React.ComponentProps<typeof AriaTabPanel>): React.ReactElement {
  return (
    <AriaTabPanel
      data-slot="tabs-content"
      className={composeRenderProps(className, (className) => cn("flex-1 text-sm outline-none", className))}
      {...props}
    />
  );
}

// oxlint-disable-next-line react/only-export-components -- shadcn pattern: cva const + component co-located.
export { Tabs, TabList, Tab, TabPanel, tabsListVariants };
