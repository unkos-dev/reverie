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
  "group/tabs-list inline-flex w-fit items-center justify-center rounded-lg p-[3px] text-muted-foreground group-data-[orientation=horizontal]/tabs:h-8 group-data-[orientation=vertical]/tabs:h-fit group-data-[orientation=vertical]/tabs:flex-col data-[variant=line]:rounded-none",
  {
    variants: {
      variant: {
        default: "bg-muted",
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
        "relative inline-flex h-[calc(100%-1px)] flex-1 items-center justify-center gap-1.5 rounded-md border border-transparent px-1.5 py-0.5 text-sm font-medium whitespace-nowrap text-foreground/60 transition-all group-data-[orientation=vertical]/tabs:w-full group-data-[orientation=vertical]/tabs:justify-start hover:text-foreground data-focus-visible:border-ring data-focus-visible:ring-[3px] data-focus-visible:ring-ring/50 data-focus-visible:outline-1 data-focus-visible:outline-ring data-disabled:pointer-events-none data-disabled:opacity-50 has-data-[icon=inline-end]:pr-1 has-data-[icon=inline-start]:pl-1 dark:text-muted-foreground dark:hover:text-foreground group-data-[variant=default]/tabs-list:data-selected:shadow-sm group-data-[variant=line]/tabs-list:data-selected:shadow-none [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
        "group-data-[variant=line]/tabs-list:bg-transparent group-data-[variant=line]/tabs-list:data-selected:bg-transparent dark:group-data-[variant=line]/tabs-list:data-selected:border-transparent dark:group-data-[variant=line]/tabs-list:data-selected:bg-transparent",
        "data-selected:bg-background data-selected:text-foreground dark:data-selected:border-input dark:data-selected:bg-input/30 dark:data-selected:text-foreground",
        "after:absolute after:bg-foreground after:opacity-0 after:transition-opacity group-data-[orientation=horizontal]/tabs:after:inset-x-0 group-data-[orientation=horizontal]/tabs:after:bottom-[-5px] group-data-[orientation=horizontal]/tabs:after:h-0.5 group-data-[orientation=vertical]/tabs:after:inset-y-0 group-data-[orientation=vertical]/tabs:after:-right-1 group-data-[orientation=vertical]/tabs:after:w-0.5 group-data-[variant=line]/tabs-list:data-selected:after:opacity-100",
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
