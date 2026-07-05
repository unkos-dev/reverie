/**
 * Perf instrumentation for the grid harness (design D5).
 *
 * Pure statistics plus a rAF frame monitor. The harness feeds keystroke and
 * mount timings in; this module summarizes them. Hard budgets are read off the
 * harness UI, never asserted in CI (perf is flaky by construction), so only the
 * pure helpers here carry unit tests.
 */

export type Summary = {
  count: number;
  p50: number;
  p95: number;
  max: number;
};

/** Nearest-rank percentile over an already-sorted-ascending sample array. */
export function percentile(sortedAsc: readonly number[], p: number): number {
  if (sortedAsc.length === 0) return 0;
  const rank = Math.ceil((p / 100) * sortedAsc.length);
  const index = Math.min(sortedAsc.length - 1, Math.max(0, rank - 1));
  return sortedAsc[index] ?? 0;
}

/** p50/p95/max summary of an unordered sample set (milliseconds). */
export function summarize(samples: readonly number[]): Summary {
  if (samples.length === 0) return { count: 0, p50: 0, p95: 0, max: 0 };
  const sorted = [...samples].sort((a, b) => a - b);
  return {
    count: sorted.length,
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    max: sorted[sorted.length - 1] ?? 0,
  };
}

export type FrameStats = {
  frames: number;
  maxFrameMs: number;
  /** Frames whose gap exceeded the stall threshold. */
  stalls: number;
};

/**
 * Watches `requestAnimationFrame` deltas while running. A gap over
 * `stallThresholdMs` (default 100 ms, the design's scroll budget) counts as a
 * stall. Browser-only; the harness starts it around a scripted scroll.
 */
export class FrameMonitor {
  private readonly stallThresholdMs: number;
  private running = false;
  private rafId = 0;
  private lastTs = 0;
  private stats: FrameStats = { frames: 0, maxFrameMs: 0, stalls: 0 };

  constructor(stallThresholdMs = 100) {
    this.stallThresholdMs = stallThresholdMs;
  }

  start(): void {
    if (this.running) return;
    this.running = true;
    this.lastTs = 0;
    this.stats = { frames: 0, maxFrameMs: 0, stalls: 0 };
    const tick = (ts: number): void => {
      if (!this.running) return;
      if (this.lastTs !== 0) {
        const delta = ts - this.lastTs;
        this.stats.frames += 1;
        if (delta > this.stats.maxFrameMs) this.stats.maxFrameMs = delta;
        if (delta > this.stallThresholdMs) this.stats.stalls += 1;
      }
      this.lastTs = ts;
      this.rafId = requestAnimationFrame(tick);
    };
    this.rafId = requestAnimationFrame(tick);
  }

  stop(): FrameStats {
    this.running = false;
    if (this.rafId !== 0) cancelAnimationFrame(this.rafId);
    this.rafId = 0;
    return this.stats;
  }
}
