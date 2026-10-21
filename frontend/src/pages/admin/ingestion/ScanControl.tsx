import { type ReactElement } from "react";
import { Loader2, RefreshCw } from "lucide-react";

import { Button } from "@/components/ui/button";

type ScanControlProps = {
  pending: boolean;
  forbidden: boolean;
  onScan: () => void;
};

/** The page's one Primary button, with its hint beneath. */
function ScanControl({ pending, forbidden, onScan }: Readonly<ScanControlProps>): ReactElement {
  return (
    <div className="flex flex-col items-end gap-1.5">
      <Button type="button" isDisabled={forbidden} isPending={pending} onPress={onScan}>
        {pending ? (
          <Loader2 aria-hidden="true" className="motion-safe:animate-spin" />
        ) : (
          <RefreshCw aria-hidden="true" />
        )}
        {pending ? "Scanning" : "Scan ingestion folder"}
      </Button>
      <span className="text-fg-muted text-[13px]">Checks every file in the folder.</span>
    </div>
  );
}

export { ScanControl };
