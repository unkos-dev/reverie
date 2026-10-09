import { type ReactElement } from "react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

function GroupLoading({ label }: { label: string }): ReactElement {
  return (
    <div aria-busy="true" className="grid gap-3 px-4 py-4 pl-12">
      <span role="status" className="sr-only">
        {label}
      </span>
      <Skeleton className="bg-surface-3 h-3 w-1/2" />
      <Skeleton className="bg-surface-3 h-3 w-2/3" />
    </div>
  );
}

function GroupFailed({ message, onRetry }: { message: string; onRetry: () => void }): ReactElement {
  return (
    <div className="text-fg-muted flex items-center justify-between gap-4 px-4 py-2 pl-12 text-[13px]">
      <span role="alert">{message}</span>
      <Button type="button" variant="outline" size="sm" onClick={onRetry}>
        Try again
      </Button>
    </div>
  );
}

export { GroupLoading, GroupFailed };
