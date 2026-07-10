/**
 * react-data-grid binding of the shared `GridBindingProps<R>` contract.
 *
 * This is the only file in `frontend/src` allowed to import `react-data-grid`;
 * everything else in the app depends on `./types`, never on RDG's own types.
 * RDG ships native ARIA grid semantics and CSS-variable theming; the theme
 * half of the bridge is `grid-theme.css`, imported here alongside RDG's own
 * stylesheet. `RenderEditCellProps` types the editor-props translation below
 * but, like every other RDG type, never appears in this module's exports.
 */
import { Info } from "lucide-react";
import { useId, useMemo, type MouseEvent, type ReactElement, type ReactNode } from "react";
import {
  DataGrid,
  renderHeaderCell,
  renderSortIcon,
  type Column,
  type RenderEditCellProps,
  type RenderHeaderCellProps,
  type RenderSortStatusProps,
  type SortColumn,
} from "react-data-grid";
import "react-data-grid/lib/styles.css";
import "./grid-theme.css";

import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

import type { GridBindingProps } from "./types";

const ROW_HEIGHT = 36;
const HEADER_HEIGHT = 40;

/**
 * Overrides RDG's default sort-status renderer to add an accessible label
 * next to the bare priority number: the vendor's own render leaves the
 * digit unlabeled, so a screen reader announces "2" with no indication
 * it names a sort priority.
 */
function renderSortStatus({ sortDirection, priority }: RenderSortStatusProps): ReactNode {
  return (
    <>
      {renderSortIcon({ sortDirection })}
      {priority === undefined ? null : (
        <span>
          {priority}
          <span className="sr-only">{` sort level ${String(priority)}`}</span>
        </span>
      )}
    </>
  );
}

/**
 * The RDG binding's own props: the shared contract plus a row-identity
 * extractor. RDG needs a stable per-row key for its keyed reconciliation, but
 * the shared contract makes no assumption about what shape `R` is, so the
 * extractor is a binding-specific addition rather than part of `GridBindingProps`.
 */
export type ReactDataGridBindingProps<R> = GridBindingProps<R> & {
  rowKey: (row: R) => string;
};

/**
 * Header cell with a tooltip-bearing info control after the default header
 * content (text, sort arrow, priority badge). The control stays out of the
 * grid's roving focus order (`tabIndex={-1}`): RDG redirects header-cell
 * focus to any `tabindex="0"` child, and a focused control would swallow
 * the Enter/Space that RDG's columnheader wrapper needs for keyboard
 * sorting. The tooltip is a pointer affordance; keyboard and screen-reader
 * users get the same note through `aria-describedby` on the columnheader,
 * announced with the header on focus. Click still must not sort, so the
 * control stops click propagation. The icon is presentation-only so the
 * column's accessible name stays the header text alone.
 */
function HeaderCellWithTooltip<R>(
  props: RenderHeaderCellProps<R> & { tooltip: { label: string; content: string } },
): ReactElement {
  const { tooltip, ...headerProps } = props;
  const descriptionId = useId();
  return (
    <span
      className="flex items-center gap-1"
      // RDG renders the columnheader wrapper itself and offers no ARIA
      // passthrough, so the description association is attached via the DOM.
      ref={(node) => {
        const header = node?.closest('[role="columnheader"]');
        if (header == null) return;
        header.setAttribute("aria-describedby", descriptionId);
        return () => {
          header.removeAttribute("aria-describedby");
        };
      }}
    >
      {renderHeaderCell(headerProps)}
      {/* `hidden` keeps the note out of the header's name-from-contents;
          aria-describedby still resolves hidden reference targets. */}
      <span id={descriptionId} hidden>
        {tooltip.content}
      </span>
      <Tooltip>
        <TooltipTrigger
          tabIndex={-1}
          aria-label={tooltip.label}
          className="text-fg-muted hover:text-fg focus-visible:ring-accent flex min-h-6 min-w-6 items-center justify-center rounded-sm focus-visible:outline-none focus-visible:ring-2"
          onClick={(event: MouseEvent) => {
            event.stopPropagation();
          }}
        >
          <Info className="size-3.5" aria-hidden="true" />
        </TooltipTrigger>
        <TooltipContent>{tooltip.content}</TooltipContent>
      </Tooltip>
    </span>
  );
}

function toRdgColumns<R>(columns: GridBindingProps<R>["columns"]): readonly Column<R>[] {
  return columns.map((col) => {
    // Captured in a local so the undefined check below narrows it for the
    // closure; a non-null assertion on `col.renderEditCell` is forbidden.
    const renderEditCell = col.renderEditCell;
    const editFields =
      renderEditCell === undefined
        ? {}
        : {
            editable: col.editable,
            editorOptions: col.editorOptions,
            renderEditCell: ({ row, onRowChange, onClose }: RenderEditCellProps<R>): ReactNode =>
              renderEditCell({
                row,
                update: (next: R) => {
                  onRowChange(next);
                },
                commit: (next: R) => {
                  onRowChange(next, true);
                },
                cancel: () => {
                  onClose();
                },
              }),
          };
    const tooltip = col.headerTooltip;
    const headerFields =
      tooltip === undefined
        ? {}
        : {
            renderHeaderCell: (props: RenderHeaderCellProps<R>): ReactNode => (
              <HeaderCellWithTooltip {...props} tooltip={tooltip} />
            ),
          };
    return {
      key: col.key,
      name: col.name,
      sortable: col.sortable,
      width: col.width,
      renderCell: ({ row }: { row: R }): ReactNode =>
        col.renderCell === undefined ? col.accessor(row) : col.renderCell(row),
      ...headerFields,
      ...editFields,
    };
  });
}

export function ReactDataGridBinding<R>(props: ReactDataGridBindingProps<R>): ReactElement {
  const {
    rows,
    columns,
    label,
    sort,
    onSortChange,
    onCellFocus,
    onCellEdit,
    onScroll,
    rowKey,
    className,
    height,
  } = props;

  const rdgColumns = useMemo(() => toRdgColumns(columns), [columns]);

  const sortColumns: readonly SortColumn[] = sort.map((level) => ({
    columnKey: level.columnKey,
    direction: level.direction === "asc" ? "ASC" : "DESC",
  }));

  function handleSortColumnsChange(next: SortColumn[]): void {
    onSortChange(
      next.map((col) => ({
        columnKey: col.columnKey,
        direction: col.direction === "ASC" ? "asc" : "desc",
      })),
    );
  }

  const wrapperClass = className === undefined ? "rv-grid" : `rv-grid ${className}`;

  return (
    // Height is a dynamic, prop-driven scroll viewport (cardinal-rule
    // exception); when omitted the caller's className must size the wrapper.
    // The tooltip provider is mounted here because header tooltips are the
    // binding's own affordance and no app-level provider exists; Radix
    // Tooltip.Root throws without a provider ancestor.
    <TooltipProvider>
      <div className={wrapperClass} style={height === undefined ? undefined : { height }}>
        <DataGrid
          aria-label={label}
          columns={rdgColumns}
          rows={rows}
          rowKeyGetter={rowKey}
          sortColumns={sortColumns}
          onSortColumnsChange={handleSortColumnsChange}
          onSelectedCellChange={({ row, rowIdx, column }) => {
            // Header-row selection reports no row object; only cell focus does.
            if (row === undefined) return;
            onCellFocus({ row, rowIdx, columnKey: column.key });
          }}
          onRowsChange={(nextRows, { indexes, column }) => {
            // Fill/paste touch multiple rows in one event; bulk editing is a
            // later tranche, so multi-index commits are deliberately dropped.
            if (indexes.length !== 1 || onCellEdit === undefined) return;
            const index = indexes[0];
            onCellEdit({ row: nextRows[index], previousRow: rows[index], columnKey: column.key });
          }}
          onScroll={onScroll}
          rowHeight={ROW_HEIGHT}
          headerRowHeight={HEADER_HEIGHT}
          renderers={{ renderSortStatus }}
        />
      </div>
    </TooltipProvider>
  );
}
