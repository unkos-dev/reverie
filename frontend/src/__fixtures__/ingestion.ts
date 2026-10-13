import type { AuthMe } from "@/hooks/useAuthMe";

import { STUB_ME } from "./auth";
import type { DashboardActivity, DashboardStats } from "@/api/dashboard";
import type { FailureItem, FailureCounts } from "@/api/enrichment-failures";
import type { InputCounts, InputItem } from "@/api/ingestion";

const ADMIN_ME: AuthMe = STUB_ME;
const ADULT_ME: AuthMe = { ...STUB_ME, role: "adult" };
const CHILD_ME: AuthMe = { ...ADULT_ME, is_child: true };

function uuid(n: number): string {
  return `019a0000-0000-7000-8000-${String(n).padStart(12, "0")}`;
}

function inputItem(n: number, overrides: Partial<InputItem> = {}): InputItem {
  return {
    id: uuid(n),
    path: `incoming/Book ${String(n)}.epub`,
    status: "rejected",
    outcome: "rejected",
    primary_reason: "damaged",
    reasons: ["damaged"],
    observed_at: "2026-10-09T08:00:00Z",
    completed_at: "2026-10-09T08:12:00Z",
    ...overrides,
  };
}

function countsFixture(overrides: Partial<InputCounts> = {}): InputCounts {
  return {
    by_reason: [
      { reason: "damaged", count: 2 },
      { reason: "invalid_structure", count: 1 },
      { reason: "unspecified", count: 1 },
      { reason: "needs_change", count: 2 },
      { reason: "retries_exhausted", count: 1 },
      { reason: "format_not_accepted", count: 14 },
    ],
    attention_total: 7,
    ...overrides,
  };
}

function failureItem(n: number, overrides: Partial<FailureItem> = {}): FailureItem {
  return {
    manifestation_id: uuid(1000 + n),
    work_id: uuid(2000 + n),
    title: `Book title ${String(n)}`,
    status: "failed",
    attempt_count: 3,
    attempted_at: "2099-01-01T00:00:00Z",
    primary: { source: "openlibrary", class: "timeout" },
    also: [],
    ...overrides,
  };
}

function failureCountsFixture(): FailureCounts {
  return {
    by_failure: [
      { source: "hardcover", class: "not_found", count: 1 },
      { source: "openlibrary", class: "timeout", count: 2 },
      { source: null, class: "unspecified", count: 1 },
    ],
    total: 4,
  };
}

function batch(overrides: Partial<DashboardActivity["batches"][number]> = {}): DashboardActivity {
  return {
    batches: [
      {
        batch_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        started_at: "2026-10-09T08:00:00Z",
        ended_at: "2026-10-09T08:02:11Z",
        total: 12,
        completed: 11,
        failed: 1,
        skipped: 0,
        in_progress: 0,
        ...overrides,
      },
    ],
  };
}

function statsFixture(): DashboardStats {
  return {
    total_manifestations: 1204,
    total_works: 980,
    storage_total_bytes: 3_650_000_000,
    storage_cover_bytes: 41_000_000,
    storage_by_format: [],
    validation_breakdown: [],
    clean_non_epub_count: 0,
    enrichment_breakdown: [],
    metadata_coverage: {
      total: 0,
      has_description: 0,
      has_language: 0,
      has_isbn_13: 0,
      has_cover: 0,
    },
  };
}

export {
  ADMIN_ME,
  ADULT_ME,
  CHILD_ME,
  batch,
  countsFixture,
  failureCountsFixture,
  failureItem,
  inputItem,
  statsFixture,
  uuid,
};
