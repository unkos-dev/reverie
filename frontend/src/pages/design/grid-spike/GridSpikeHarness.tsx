/**
 * Grid perf harness (dev-only, `/design/grid-spike`).
 *
 * Drives the production `ReactDataGridBinding` (the bake-off winner, promoted
 * to `@/lib/grid`) over a synthetic in-memory library and reports the design
 * D5 budgets (keystroke-to-cell-move p50/p95/max, scroll frame-stalls, grid
 * mount) at row counts the live keyset API cannot exercise on one screen.
 * Numbers are read here, in the browser, during a scripted QA session; none
 * are asserted in CI. Data comes from the in-memory mock consumed through the
 * same `useSuspenseInfiniteQuery` shape `LibraryPage` uses. Read-only: there
 * is no edit hook in the promoted contract, so the keystroke bench measures
 * navigation only.
 */
import { useSuspenseInfiniteQuery, type InfiniteData } from "@tanstack/react-query";
import {
  Suspense,
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
} from "react";
import { useRouteError } from "react-router";

import { ThemeSwitcher } from "@/components/theme-switcher";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  ReactDataGridBinding,
  type ReactDataGridBindingProps,
} from "@/lib/grid/ReactDataGridBinding";
import type { FocusReport, SortState } from "@/lib/grid/types";

import { SPIKE_COLUMNS } from "./columns";
import { DEFAULT_ROW_COUNT, generateRows } from "./data/generator";
import { LATENCY_PRESETS, listSpikeBooks, type SpikeListResponse } from "./data/mock-api";
import { recordMove } from "./harness-helpers";
import { FrameMonitor, summarize, type FrameStats, type Summary } from "./perf/instrument";
import type { SpikeBookRow } from "./types";

const SEED = 20260704;
const PAGE_SIZE = 1_000;
const GRID_HEIGHT = 560;
const KEYSTROKE_MOVES = 200;
const MOVE_TIMEOUT_MS = 300;
const ROW_COUNT_PRESETS = [1_000, 10_000, DEFAULT_ROW_COUNT] as const;

function raf(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => {
      resolve();
    });
  });
}

function dispatchKey(target: Element | null, key: string, ctrlKey = false): void {
  if (target === null) return;
  target.dispatchEvent(new KeyboardEvent("keydown", { key, ctrlKey, bubbles: true }));
}

type Metrics = {
  keystroke: Summary | null;
  keystrokeDropped: number;
  frame: FrameStats | null;
  scrollNote: string | null;
  mountMs: number | null;
};

const EMPTY_METRICS: Metrics = {
  keystroke: null,
  keystrokeDropped: 0,
  frame: null,
  scrollNote: null,
  mountMs: null,
};

/** Route errorElement: keeps a harness fetch/parse failure local and readable. */
export function GridSpikeError(): ReactElement {
  const error = useRouteError();
  const message = error instanceof Error ? error.message : "Unknown harness error";
  return (
    <main className="text-fg p-8">
      <h1 className="text-lg font-semibold">Grid spike harness error</h1>
      <p className="text-fg-muted mt-2 font-mono text-sm">{message}</p>
    </main>
  );
}

export default function GridSpikeHarness(): ReactElement {
  const [latencyMs, setLatencyMs] = useState<number>(0);
  const [rowCount, setRowCount] = useState<number>(DEFAULT_ROW_COUNT);
  const [sort, setSort] = useState<SortState>(null);

  return (
    <main className="bg-canvas text-fg flex h-dvh flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center gap-3">
        <h1 className="mr-auto text-lg font-semibold">Grid perf harness</h1>
        <ThemeSwitcher />
      </header>

      <ConfigToolbar
        latencyMs={latencyMs}
        rowCount={rowCount}
        sort={sort}
        onLatencyChange={setLatencyMs}
        onRowCountChange={setRowCount}
        onResetSort={() => {
          setSort(null);
        }}
      />

      <Suspense fallback={<div className="text-fg-muted p-8">Generating {rowCount} rows…</div>}>
        <GridStage latencyMs={latencyMs} rowCount={rowCount} sort={sort} onSortChange={setSort} />
      </Suspense>
    </main>
  );
}

type ConfigToolbarProps = {
  latencyMs: number;
  rowCount: number;
  sort: SortState;
  onLatencyChange: (ms: number) => void;
  onRowCountChange: (n: number) => void;
  onResetSort: () => void;
};

function ConfigToolbar(props: ConfigToolbarProps): ReactElement {
  return (
    <section className="border-border flex flex-wrap items-center gap-4 rounded-lg border p-3 text-sm">
      <fieldset className="flex items-center gap-2">
        <span className="text-fg-muted">Latency</span>
        {LATENCY_PRESETS.map((ms) => (
          <Button
            key={ms}
            size="sm"
            variant={props.latencyMs === ms ? "default" : "outline"}
            onClick={() => {
              props.onLatencyChange(ms);
            }}
          >
            {ms} ms
          </Button>
        ))}
      </fieldset>

      <fieldset className="flex items-center gap-2">
        <span className="text-fg-muted">Rows</span>
        {ROW_COUNT_PRESETS.map((n) => (
          <Button
            key={n}
            size="sm"
            variant={props.rowCount === n ? "default" : "outline"}
            onClick={() => {
              props.onRowCountChange(n);
            }}
          >
            {n.toLocaleString()}
          </Button>
        ))}
      </fieldset>

      {props.sort !== null && (
        <Button size="sm" variant="ghost" onClick={props.onResetSort}>
          Clear sort ({props.sort.columnKey} {props.sort.direction})
        </Button>
      )}
    </section>
  );
}

type GridStageProps = {
  latencyMs: number;
  rowCount: number;
  sort: SortState;
  onSortChange: (sort: SortState) => void;
};

function GridStage(props: GridStageProps): ReactElement {
  const { latencyMs, rowCount, sort, onSortChange } = props;

  const dataset = useMemo(() => generateRows(SEED, rowCount), [rowCount]);

  const { data, fetchNextPage, hasNextPage, isFetchingNextPage } = useSuspenseInfiniteQuery<
    SpikeListResponse,
    Error,
    InfiniteData<SpikeListResponse, string | undefined>,
    readonly [string, number, number, SortState],
    string | undefined
  >({
    queryKey: ["grid-spike", latencyMs, rowCount, sort],
    queryFn: ({ pageParam, signal }) =>
      listSpikeBooks(dataset, { cursor: pageParam, pageSize: PAGE_SIZE, sort, latencyMs }, signal),
    initialPageParam: undefined,
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? undefined,
  });

  const rows: readonly SpikeBookRow[] = useMemo(
    () => data.pages.flatMap((p) => p.items),
    [data.pages],
  );

  const stageRef = useRef<HTMLDivElement>(null);
  const [metrics, setMetrics] = useState<Metrics>(EMPTY_METRICS);
  const [running, setRunning] = useState(false);
  const [remountKey, setRemountKey] = useState(0);

  // Keystroke bench plumbing: the binding's focus report resolves the pending
  // move after the next paint, yielding keydown-to-cell-move deltas.
  const benchActiveRef = useRef(false);
  const moveStartRef = useRef(0);
  const moveResolveRef = useRef<(() => void) | null>(null);
  const samplesRef = useRef<number[]>([]);
  const mountStartRef = useRef(0);

  const handleCellFocus = useCallback((_report: FocusReport<SpikeBookRow>): void => {
    if (!benchActiveRef.current) return;
    requestAnimationFrame(() => {
      const resolve = moveResolveRef.current;
      // The move already settled (timed out and was recorded by the loop). Drop
      // the late focus so it cannot double-count into the sample.
      if (resolve === null) return;
      moveResolveRef.current = null;
      recordMove(samplesRef.current, performance.now() - moveStartRef.current, MOVE_TIMEOUT_MS);
      resolve();
    });
  }, []);

  const handleMounted = useCallback((): void => {
    if (mountStartRef.current === 0) return;
    const dt = performance.now() - mountStartRef.current;
    mountStartRef.current = 0;
    setMetrics((m) => ({ ...m, mountMs: dt }));
  }, []);

  async function loadAll(): Promise<void> {
    setRunning(true);
    let result = await fetchNextPage();
    while (result.hasNextPage) result = await fetchNextPage();
    setRunning(false);
  }

  async function runKeystrokeBench(): Promise<void> {
    setRunning(true);
    samplesRef.current = [];
    benchActiveRef.current = true;

    // RIG FIX (limitation 1): the bench used to "focus" the grid with a
    // synthetic MouseEvent click, which moves neither react-data-grid's
    // internal selection nor `document.activeElement` -- RDG tracks the
    // selected cell from a `mousedown` listener, not `click`, and (unlike a
    // real pointer click) never calls `.focus()` on selection itself; real
    // browsers only move focus there via the native focus-follows-mousedown
    // default action. A synthetic `click` triggers neither path, so every
    // subsequent keydown fell back to the grid root (which does not
    // navigate) and timed out -- an unattended run silently dropped all
    // 200/200 moves. Dispatching `mousedown` sets RDG's selection; the
    // explicit `.focus()` call stands in for the missing native default
    // action so `document.activeElement` genuinely tracks the grid from here
    // (the cell's existing `tabindex` already permits programmatic focus, so
    // no roving-tabindex value is overwritten).
    const gridEl = stageRef.current?.querySelector('[role="grid"]') ?? null;
    const firstCell = stageRef.current?.querySelector('[role="gridcell"]') ?? null;
    firstCell?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    if (firstCell instanceof HTMLElement) firstCell.focus();
    await raf();

    const keys = ["ArrowDown", "ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"];
    let dropped = 0;
    for (let i = 0; i < KEYSTROKE_MOVES; i += 1) {
      const target = document.activeElement ?? gridEl;
      moveStartRef.current = performance.now();
      const moved = new Promise<void>((resolve) => {
        moveResolveRef.current = resolve;
      });
      let timer = 0;
      const timeout = new Promise<void>((resolve) => {
        timer = window.setTimeout(resolve, MOVE_TIMEOUT_MS);
      });
      dispatchKey(target, keys[i % keys.length] ?? "ArrowDown");
      await Promise.race([moved, timeout]);
      window.clearTimeout(timer);
      // Timeout won: focus never arrived. Record the ceiling so the stall counts
      // against the budget instead of vanishing from the sample.
      if (moveResolveRef.current !== null) {
        moveResolveRef.current = null;
        if (recordMove(samplesRef.current, null, MOVE_TIMEOUT_MS)) dropped += 1;
      }
    }

    benchActiveRef.current = false;
    setMetrics((m) => ({
      ...m,
      keystroke: summarize(samplesRef.current),
      keystrokeDropped: dropped,
    }));
    setRunning(false);
  }

  async function runScrollBench(): Promise<void> {
    // RIG FIX (limitation 2): the AG Grid candidate is gone, so the stale
    // '.ag-body-viewport' half of this selector could only ever mask a
    // missing scroller as a silent no-op. `.rdg` is the promoted binding's
    // own scroll container; the "not found" branch below stays a reported
    // error state rather than a swallowed failure.
    const scroller = stageRef.current?.querySelector(".rdg") ?? null;
    if (!(scroller instanceof HTMLElement)) {
      setMetrics((m) => ({ ...m, frame: null, scrollNote: "scroll container not found" }));
      return;
    }
    setRunning(true);
    setMetrics((m) => ({ ...m, scrollNote: null }));
    const monitor = new FrameMonitor(100);
    monitor.start();
    const started = performance.now();
    while (performance.now() - started < 1_200) {
      scroller.scrollTop += 400;
      await raf();
    }
    const gridEl = stageRef.current?.querySelector('[role="grid"]') ?? null;
    dispatchKey(document.activeElement ?? gridEl, "End", true);
    scroller.scrollTop = scroller.scrollHeight;
    await raf();
    await raf();
    setMetrics((m) => ({ ...m, frame: monitor.stop() }));
    setRunning(false);
  }

  function measureMount(): void {
    mountStartRef.current = performance.now();
    setRemountKey((k) => k + 1);
  }

  const bindingProps: Omit<ReactDataGridBindingProps<SpikeBookRow>, "className"> = {
    rows,
    columns: SPIKE_COLUMNS,
    rowKey: (row) => row.id,
    sort,
    onSortChange,
    onCellFocus: handleCellFocus,
    height: GRID_HEIGHT,
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <BenchBar
        running={running}
        rowsLoaded={rows.length}
        totalRows={rowCount}
        hasNextPage={hasNextPage}
        isFetchingNextPage={isFetchingNextPage}
        onLoadAll={() => {
          void loadAll();
        }}
        onKeystroke={() => {
          void runKeystrokeBench();
        }}
        onScroll={() => {
          void runScrollBench();
        }}
        onMount={measureMount}
      />
      <MetricsPanel metrics={metrics} />
      <div ref={stageRef} className="min-h-0 flex-1">
        <GridView key={remountKey} bindingProps={bindingProps} onMounted={handleMounted} />
      </div>
    </div>
  );
}

type GridViewProps = {
  bindingProps: Omit<ReactDataGridBindingProps<SpikeBookRow>, "className">;
  onMounted: () => void;
};

/** Isolates one grid mount so `measureMount` can time init in a layout effect. */
function GridView({ bindingProps, onMounted }: GridViewProps): ReactElement {
  useLayoutEffect(() => {
    onMounted();
  }, [onMounted]);
  return <ReactDataGridBinding {...bindingProps} />;
}

type BenchBarProps = {
  running: boolean;
  rowsLoaded: number;
  totalRows: number;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  onLoadAll: () => void;
  onKeystroke: () => void;
  onScroll: () => void;
  onMount: () => void;
};

function BenchBar(props: BenchBarProps): ReactElement {
  return (
    <section className="border-border flex flex-wrap items-center gap-3 rounded-lg border p-3 text-sm">
      <Button size="sm" disabled={props.running || !props.hasNextPage} onClick={props.onLoadAll}>
        {props.isFetchingNextPage ? "Loading…" : "Load all pages"}
      </Button>
      <Button size="sm" variant="outline" disabled={props.running} onClick={props.onKeystroke}>
        Keystroke bench ({KEYSTROKE_MOVES} moves)
      </Button>
      <Button size="sm" variant="outline" disabled={props.running} onClick={props.onScroll}>
        Scroll + Ctrl+End
      </Button>
      <Button size="sm" variant="outline" disabled={props.running} onClick={props.onMount}>
        Measure mount
      </Button>
      <Badge variant="outline" className="ml-auto">
        {props.rowsLoaded.toLocaleString()} / {props.totalRows.toLocaleString()} rows
      </Badge>
    </section>
  );
}

function keystrokeText(keystroke: Summary, dropped: number): string {
  const base = `p50 ${keystroke.p50.toFixed(1)} · p95 ${keystroke.p95.toFixed(1)} · max ${keystroke.max.toFixed(1)} ms (n=${String(keystroke.count)})`;
  return dropped === 0
    ? base
    : `${base} · dropped ${String(dropped)}/${String(KEYSTROKE_MOVES)} (recorded at ${String(MOVE_TIMEOUT_MS)} ms ceiling)`;
}

function scrollText(frame: FrameStats | null, note: string | null): string {
  if (note !== null) return note;
  if (frame === null) return "—";
  return `max frame ${frame.maxFrameMs.toFixed(1)} ms · stalls ${String(frame.stalls)} / ${String(frame.frames)} frames`;
}

function MetricsPanel({ metrics }: { metrics: Metrics }): ReactElement {
  const { keystroke, keystrokeDropped, frame, scrollNote, mountMs } = metrics;
  return (
    <section className="border-border grid grid-cols-1 gap-3 rounded-lg border p-3 text-sm sm:grid-cols-3">
      <Metric label="Keystroke to cell-move (budget p95 ≤ 33 ms)">
        {keystroke === null ? "—" : keystrokeText(keystroke, keystrokeDropped)}
      </Metric>
      <Metric label="Scroll frames (budget: no stall > 100 ms)">
        {scrollText(frame, scrollNote)}
      </Metric>
      <Metric label="Grid mount (budget < 1000 ms)">
        {mountMs === null ? "—" : `${mountMs.toFixed(1)} ms`}
      </Metric>
    </section>
  );
}

function Metric({
  label,
  children,
}: {
  label: string;
  children: ReactElement | string;
}): ReactElement {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-fg-muted text-xs">{label}</span>
      <span className="font-mono">{children}</span>
    </div>
  );
}
