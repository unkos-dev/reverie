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
import {
  useId,
  useMemo,
  type ChangeEvent,
  type MouseEvent,
  type ReactElement,
  type ReactNode,
} from "react";
import {
  DataGrid,
  SelectColumn,
  renderHeaderCell,
  renderSortIcon,
  type CellKeyboardEvent,
  type CellKeyDownArgs,
  type CellMouseArgs,
  type CellMouseEvent,
  type Column,
  type RenderCheckboxProps,
  type RenderEditCellProps,
  type RenderHeaderCellProps,
  type RenderSortStatusProps,
  type SortColumn,
} from "react-data-grid";
import "react-data-grid/lib/styles.css";
import "./grid-theme.css";

import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";

import type { GridBindingProps, GridColumn } from "./types";

const ROW_HEIGHT = 36;
const HEADER_HEIGHT = 40;

/**
 * Elements whose clicks and keys belong to themselves, never to row
 * activation: links, buttons, form controls, and anything inside them.
 */
function isInteractiveTarget(target: EventTarget | null): boolean {
  return (
    target instanceof Element &&
    target.closest("a,button,input,textarea,select,[role='button']") !== null
  );
}

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
  props: RenderHeaderCellProps<R> & {
    tooltip: { label: string; content: string };
  },
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

/**
 * Checkbox renderer that keeps the vendor's behaviour (indeterminate via
 * ref, shift-aware change reporting for range select) while letting the
 * caller name the header select-all control. The header instance is the
 * only one handed `indeterminate`, which is how it is distinguished here.
 */
function makeRenderCheckbox(selectAllLabel: string) {
  return function renderCheckbox({
    indeterminate,
    onChange,
    ...checkboxProps
  }: RenderCheckboxProps): ReactNode {
    const isHeader = indeterminate !== undefined;
    function handleChange(event: ChangeEvent<HTMLInputElement>): void {
      const native = event.nativeEvent;
      const shiftKey = native instanceof globalThis.MouseEvent ? native.shiftKey : false;
      onChange(event.target.checked, shiftKey);
    }
    return (
      <input
        type="checkbox"
        {...checkboxProps}
        aria-label={isHeader ? selectAllLabel : checkboxProps["aria-label"]}
        ref={(el) => {
          if (el !== null) el.indeterminate = indeterminate === true;
        }}
        onChange={handleChange}
      />
    );
  };
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
    onRowActivate,
    selection,
    onScroll,
    rowKey,
    className,
    height,
    rowHeight,
    headerRowHeight,
  } = props;

  const hasSelection = selection !== undefined;
  const rdgColumns = useMemo(() => {
    const base = toRdgColumns(columns);
    return hasSelection ? [SelectColumn, ...base] : base;
  }, [columns, hasSelection]);

  const columnByKey = useMemo(() => {
    return new Map<string, GridColumn<R>>(columns.map((col) => [col.key, col]));
  }, [columns]);

  function isEditorColumn(columnKey: string): boolean {
    return columnByKey.get(columnKey)?.renderEditCell !== undefined;
  }

  /**
   * Activation guard shared by click and keyboard paths: the selection
   * column, interactive content, and editor-bearing columns all keep
   * their own gestures (activation must never shadow editing or
   * selection). The guard keys on editor presence, not the per-row
   * `editable` gate: that gate also locks cells transiently (an
   * in-flight commit), and a cell must not flip into a drawer trigger
   * mid-edit-flow.
   */
  function activationRow(
    args: { column: { key: string } | undefined; row: R | undefined },
    target: EventTarget | null,
  ): R | null {
    if (onRowActivate === undefined) return null;
    // Header, summary, and between-cell ACTIVE positions report no row or
    // column; activation applies only to a real body cell.
    if (args.row == null || args.column === undefined) return null;
    if (args.column.key === SelectColumn.key) return null;
    if (isInteractiveTarget(target)) return null;
    if (isEditorColumn(args.column.key)) return null;
    return args.row;
  }

  function handleCellClick(args: CellMouseArgs<R>, event: CellMouseEvent): void {
    const row = activationRow(args, event.target);
    if (row === null) return;
    onRowActivate?.(row);
  }

  function handleCellKeyDown(args: CellKeyDownArgs<R>, event: CellKeyboardEvent): void {
    if (args.mode === "EDIT") return;
    if (event.key !== "Enter" && event.key !== " ") return;
    // Shift+Space is the vendor's built-in row-selection toggle.
    if (event.key === " " && event.shiftKey) return;
    const row = activationRow(args, event.target);
    if (row === null) return;
    event.preventGridDefault();
    event.preventDefault();
    onRowActivate?.(row);
  }

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
          onActivePositionChange={({ row, rowIdx, column }) => {
            // Header and summary rows, and row-level (non-cell) positions,
            // report no row or column; cell focus needs both.
            if (row === undefined || column === undefined) return;
            onCellFocus({ row, rowIdx, columnKey: column.key });
          }}
          onRowsChange={(nextRows, { indexes, column }) => {
            // Fill/paste touch multiple rows in one event; bulk editing is a
            // later tranche, so multi-index commits are deliberately dropped.
            if (indexes.length !== 1 || onCellEdit === undefined) return;
            const index = indexes[0];
            onCellEdit({
              row: nextRows[index],
              previousRow: rows[index],
              columnKey: column.key,
            });
          }}
          onScroll={onScroll}
          onCellClick={onRowActivate === undefined ? undefined : handleCellClick}
          onCellKeyDown={onRowActivate === undefined ? undefined : handleCellKeyDown}
          selectedRows={selection?.selectedKeys}
          onSelectedRowsChange={
            selection === undefined
              ? undefined
              : (next) => {
                  selection.onSelectionChange(next);
                }
          }
          rowHeight={rowHeight ?? ROW_HEIGHT}
          headerRowHeight={headerRowHeight ?? HEADER_HEIGHT}
          renderers={{
            renderSortStatus,
            ...(selection === undefined
              ? {}
              : {
                  renderCheckbox: makeRenderCheckbox(selection.selectAllLabel),
                }),
          }}
        />
      </div>
    </TooltipProvider>
  );
}
