import { type ReactElement, type ReactNode } from "react";
import { ChevronDown } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";

import type { InputStatusTag } from "./reason-copy";

type GroupShellProps = {
  title: string;
  action: string;
  tag?: InputStatusTag;
  /** Already formatted, for example `"2 files"`. */
  countLabel: string;
  muted?: boolean;
  defaultOpen: boolean;
  /** Mounted only while the group is open, so a closed group makes no requests. */
  children: ReactNode;
};

function StatusTag({ tag }: { tag: InputStatusTag }): ReactElement {
  return (
    <Badge
      variant="outline"
      className={cn(
        "border-border-strong text-fg-muted rounded-sm px-1.5 font-mono text-[11px] font-normal tracking-wider uppercase",
        tag === "Rejected" && "border-fg-muted text-fg font-bold",
      )}
    >
      {tag}
    </Badge>
  );
}

function GroupShell({
  title,
  action,
  tag,
  countLabel,
  muted = false,
  defaultOpen,
  children,
}: GroupShellProps): ReactElement {
  return (
    <Collapsible defaultOpen={defaultOpen} className="border-border border-t">
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="group/trigger hover:bg-surface-2/60 grid w-full grid-cols-[20px_minmax(0,1fr)_auto] items-start gap-3 px-4 py-3.5 text-left"
        >
          <ChevronDown
            aria-hidden="true"
            className="text-fg-muted mt-1 size-4 transition-transform group-data-[state=closed]/trigger:-rotate-90"
          />
          <span className="flex flex-col gap-0.5">
            <strong
              className={cn("text-[15px]", muted ? "text-fg-muted font-medium" : "font-bold")}
            >
              {title}
            </strong>
            <span className="text-fg-muted text-[13px]">{action}</span>
          </span>
          <span className="flex items-center gap-2">
            {tag ? <StatusTag tag={tag} /> : null}
            <span className="font-mono text-[13px]">{countLabel}</span>
          </span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-border bg-canvas-2/55 border-t">
        {children}
      </CollapsibleContent>
    </Collapsible>
  );
}

export { GroupShell };
