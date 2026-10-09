import { type ReactElement, useEffect, useRef } from "react";
import { Loader2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { formatCount } from "./format";
import type { PagedGroup } from "./use-paged-group";

type GroupFooterProps = {
  group: Pick<
    PagedGroup<unknown>,
    "rows" | "hasMore" | "isLoadingMore" | "isMoreError" | "loadedMore" | "showMore" | "retry"
  >;
  total: number;
  /** `"files"` or `"books"`, for the announcement. */
  noun: string;
};

/** "Showing n of total", "Show more" and the page-failure state of one group. */
function GroupFooter({ group, total, noun }: GroupFooterProps): ReactElement {
  const { rows, hasMore, isLoadingMore, isMoreError, loadedMore, showMore, retry } = group;
  const moreRef = useRef<HTMLButtonElement>(null);
  const retryRef = useRef<HTMLButtonElement>(null);
  const countRef = useRef<HTMLSpanElement>(null);
  const restoreFocus = useRef(false);

  useEffect(() => {
    if (!restoreFocus.current || isLoadingMore) return undefined;
    restoreFocus.current = false;
    const active = document.activeElement;
    if (active !== null && active !== document.body) return undefined;
    const target = isMoreError ? retryRef : hasMore ? moreRef : countRef;
    target.current?.focus();
    return undefined;
  }, [isLoadingMore, isMoreError, hasMore]);

  return (
    <div className="border-border text-fg-muted flex items-center justify-between gap-4 border-t px-4 py-2 pl-12 text-[13px]">
      {isMoreError ? (
        <>
          <span role="alert">Could not load these {noun}.</span>
          <Button
            ref={retryRef}
            type="button"
            variant="outline"
            size="sm"
            onClick={() => {
              restoreFocus.current = true;
              retry();
            }}
          >
            Try again
          </Button>
        </>
      ) : (
        <>
          <span ref={countRef} tabIndex={-1}>
            Showing {rows.length} of {total}
          </span>
          {hasMore ? (
            <Button
              ref={moreRef}
              type="button"
              variant="outline"
              size="sm"
              aria-disabled={isLoadingMore}
              className={cn(isLoadingMore && "pointer-events-none opacity-50")}
              onClick={() => {
                restoreFocus.current = true;
                showMore();
              }}
            >
              {isLoadingMore ? (
                <Loader2 aria-hidden="true" className="motion-safe:animate-spin" />
              ) : null}
              {isLoadingMore ? "Loading more" : "Show more"}
            </Button>
          ) : null}
        </>
      )}
      <span className="sr-only" role="status">
        {loadedMore > 0 && !isLoadingMore
          ? `Loaded ${formatCount(loadedMore)} more ${loadedMore === 1 ? noun.slice(0, -1) : noun}`
          : ""}
      </span>
    </div>
  );
}

export { GroupFooter };
