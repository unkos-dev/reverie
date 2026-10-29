import type { AuthMe } from "@/hooks/useAuthMe";

import { STUB_ME } from "./auth";
import type { DashboardActivity } from "@/api/dashboard";
import type { InputCounts } from "@/api/ingestion";

const ADMIN_ME: AuthMe = STUB_ME;
const ADULT_ME: AuthMe = { ...STUB_ME, role: "adult" };
const CHILD_ME: AuthMe = { ...ADULT_ME, is_child: true };

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

export { ADMIN_ME, ADULT_ME, CHILD_ME, batch, countsFixture };
