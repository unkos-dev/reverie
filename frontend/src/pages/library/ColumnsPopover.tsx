/**
 * Column-visibility control for the library table: a popover with one
 * checkbox per hideable column plus a reset. Title never appears here
 * (it anchors every row), and the selection and details columns are not
 * columns in the catalog at all. Visibility is a per-user display
 * preference: the container persists it to the reader's account, never to
 * the URL.
 *
 * The reset drops the reader's override rather than clearing the set, so the
 * group inherits the installation default again. The two coincide today,
 * where nothing is hidden by default; `canReset` is what keeps the control
 * honest if that default ever changes.
 */
import { Columns3 } from "lucide-react";
import type { ReactElement } from "react";

import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";

import { HIDEABLE_COLUMNS } from "./table-columns";

type ColumnsPopoverProps = {
  hiddenColumns: ReadonlySet<string>;
  onToggleColumn: (key: string, hidden: boolean) => void;
  /** False when the visible set already matches the installation default. */
  canReset: boolean;
  onReset: () => void;
};

export function ColumnsPopover({
  hiddenColumns,
  onToggleColumn,
  canReset,
  onReset,
}: ColumnsPopoverProps): ReactElement {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button type="button" variant="outline" size="sm">
          <Columns3 className="size-4" aria-hidden="true" />
          Columns
          {hiddenColumns.size > 0 ? (
            <span className="bg-accent-soft text-fg rounded-full px-1.5 py-0.5 text-[0.65rem] leading-none">
              {hiddenColumns.size}
            </span>
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-56">
        <p className="text-fg-muted mb-3 font-mono text-xs uppercase tracking-[0.12em]">Columns</p>
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-2">
            <Checkbox id="library-column-title" checked disabled />
            <Label htmlFor="library-column-title" className="text-fg-muted">
              Title
            </Label>
          </div>
          {HIDEABLE_COLUMNS.map((column) => {
            const id = `library-column-${column.key}`;
            return (
              <div key={column.key} className="flex items-center gap-2">
                <Checkbox
                  id={id}
                  checked={!hiddenColumns.has(column.key)}
                  onCheckedChange={(checked) => {
                    onToggleColumn(column.key, checked !== true);
                  }}
                />
                <Label htmlFor={id}>{column.name}</Label>
              </div>
            );
          })}
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="mt-3 w-full"
          disabled={!canReset}
          onClick={onReset}
        >
          Reset to default
        </Button>
      </PopoverContent>
    </Popover>
  );
}
