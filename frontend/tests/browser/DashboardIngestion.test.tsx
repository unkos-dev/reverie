import { QueryClient, QueryClientProvider, focusManager } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page, userEvent, type Locator } from "vite-plus/test/browser";
import { RouterProvider, createMemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import type * as EnrichmentApi from "@/api/enrichment-failures";
import type * as IngestionApi from "@/api/ingestion";
import { getDashboardActivity, getDashboardStats } from "@/api/dashboard";
import {
  getEnrichmentFailureCounts,
  listEnrichmentFailures,
  triggerEnrichment,
  retryEnrichmentFailures,
} from "@/api/enrichment-failures";
import {
  getIngestionStatus,
  getInputCounts,
  listInputs,
  scanIngestion,
  retryInputs,
} from "@/api/ingestion";
import {
  ADMIN_ME,
  ADULT_ME,
  batch,
  countsFixture,
  failureCountsFixture,
  failureItem,
  inputItem,
  statsFixture,
} from "@/__fixtures__/ingestion";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

import { DashboardPage } from "@/pages/admin/DashboardPage";
import {
  GRACE_WINDOW_MS,
  POLL_INTERVAL_MS,
  STATUS_POLL_INTERVAL_MS,
} from "@/pages/admin/ingestion/use-ingestion-monitor";

vi.mock("@/api/dashboard", () => ({
  getDashboardStats: vi.fn(),
  getDashboardActivity: vi.fn(),
}));
vi.mock("@/api/ingestion", async (importOriginal) => ({
  ...(await importOriginal<typeof IngestionApi>()),
  listInputs: vi.fn(),
  getIngestionStatus: vi.fn(),
  getInputCounts: vi.fn(),
  scanIngestion: vi.fn(),
  retryInputs: vi.fn(),
}));
vi.mock("@/api/enrichment-failures", async (importOriginal) => ({
  ...(await importOriginal<typeof EnrichmentApi>()),
  listEnrichmentFailures: vi.fn(),
  getEnrichmentFailureCounts: vi.fn(),
  triggerEnrichment: vi.fn(),
  retryEnrichmentFailures: vi.fn(),
}));

const DAMAGED_PAGE = {
  items: [
    inputItem(1, {
      path: "incoming/Marlow - The Salt Archive.epub",
      reasons: ["damaged", "invalid_structure"],
    }),
    inputItem(2, { path: "incoming/older/Dunmore - Slow Cartography.epub" }),
  ],
  next_cursor: null,
};

async function renderPage(
  me: AuthMe = ADMIN_ME,
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
): Promise<QueryClient> {
  client.setQueryData(queryKeys.auth.me(), me);
  const router = createMemoryRouter(
    [
      { path: "/admin/dashboard", element: <DashboardPage /> },
      { path: "/b/:id", element: <div data-testid="book-page" /> },
      { path: "/library", element: <div data-testid="library-page" /> },
    ],
    { initialEntries: ["/admin/dashboard"] },
  );
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
  return client;
}

function mockHappyPath(): void {
  vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [] });
  vi.mocked(getDashboardStats).mockResolvedValue(statsFixture());
  vi.mocked(getDashboardActivity).mockResolvedValue(batch());
  vi.mocked(getInputCounts).mockResolvedValue(countsFixture());
  vi.mocked(getEnrichmentFailureCounts).mockResolvedValue(failureCountsFixture());
  vi.mocked(listInputs).mockImplementation(({ reason }) =>
    Promise.resolve(
      reason === "damaged"
        ? DAMAGED_PAGE
        : { items: [inputItem(50, { primary_reason: reason })], next_cursor: null },
    ),
  );
  vi.mocked(listEnrichmentFailures).mockResolvedValue({
    items: [failureItem(1), failureItem(2)],
    next_cursor: null,
  });
}

beforeEach(() => {
  vi.resetAllMocks();
  mockHappyPath();
});

afterEach(() => {
  focusManager.setFocused(undefined);
  vi.useRealTimers();
});

async function flush(): Promise<void> {
  {
    await vi.advanceTimersByTimeAsync(20);
  }
}

describe("scan control and notices", () => {
  test("starts idle with the Primary button, its hint and an empty status region", async () => {
    await renderPage();
    await expect.element(page.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
    await expect.element(page.getByText("Checks every file in the folder.")).toBeInTheDocument();
    const region = document.querySelector<HTMLElement>("[aria-live=polite]");
    await expect.element(region).toBeEmptyDOMElement();
  });

  test("shows the requesting state while the scan is pending", async () => {
    vi.mocked(scanIngestion).mockReturnValue(new Promise(() => {}));
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect
      .element(page.getByRole("button", { name: "Scanning" }))
      .toHaveAttribute("aria-disabled", "true");
    await expect.element(page.getByText("Scanning the ingestion folder")).toBeInTheDocument();
    await expect
      .element(page.getByText("Looking at every file. This usually takes a few seconds."))
      .toBeInTheDocument();
  });

  test("an accepted scan spells out the three counts and keeps zero figures", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 12,
      deferred: 0,
      suppressed: 5,
      monitor: "/m",
    });
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect
      .element(page.getByText("Scan started. 12 queued, 0 waiting, 5 unchanged."))
      .toBeInTheDocument();
    await expect
      .element(page.getByText(/The scan queued new or changed files\./))
      .toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
  });

  test.each([
    [
      { queued: 0, deferred: 0, suppressed: 0 },
      "Nothing to import",
      "The folder has no new or changed files.",
    ],
    [
      { queued: 0, deferred: 0, suppressed: 5 },
      "Nothing new to import",
      "5 files were left as they are, because nothing about them has changed.",
    ],
    [
      { queued: 0, deferred: 1, suppressed: 5 },
      "Nothing is ready yet",
      "1 file is waiting for readiness or a retry time. You do not need to scan again.",
    ],
    [
      { queued: 0, deferred: 3, suppressed: 0 },
      "Nothing is ready yet",
      "3 files are waiting for readiness or a retry time. You do not need to scan again.",
    ],
  ])("scan result %j shows %s", async (counts, title, detail) => {
    vi.mocked(scanIngestion).mockResolvedValue({ ...counts, monitor: "/m" });
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect.element(page.getByText(title)).toBeInTheDocument();
    await expect.element(page.getByText(detail)).toBeInTheDocument();
  });

  test("a failed request reads as a recoverable alert and offers Try again", async () => {
    vi.mocked(scanIngestion).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    vi.mocked(scanIngestion).mockResolvedValueOnce({
      queued: 0,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    const alert = page.getByRole("alert");
    await expect.element(alert.getByText("The scan did not start")).toBeInTheDocument();
    await userEvent.click(alert.getByRole("button", { name: "Try again" }));
    await expect.element(page.getByText("Nothing to import")).toBeInTheDocument();
    await expect.element(page.getByText("The scan did not start")).not.toBeInTheDocument();
  });

  test("a network failure is treated like a failed request", async () => {
    vi.mocked(scanIngestion).mockRejectedValue(new TypeError("Failed to fetch"));
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect.element(page.getByText("The scan did not start")).toBeInTheDocument();
  });

  test("a 403 disables the button and says the account cannot scan", async () => {
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(403, null, "Forbidden", ""));
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect.element(page.getByText("This account cannot start a scan")).toBeInTheDocument();
    await expect.element(page.getByRole("alert")).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Scan ingestion folder" }))
      .toBeDisabled();
  });

  test("a 401 says the session has ended", async () => {
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(401, null, "Unauthorized", ""));
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect.element(page.getByText("Your session has ended")).toBeInTheDocument();
    await expect
      .element(page.getByText("Sign in again to scan the ingestion folder."))
      .toBeInTheDocument();
  });

  test("names files sent for another try once the refetch shows fewer needs-change files", async () => {
    vi.mocked(getInputCounts).mockResolvedValueOnce(countsFixture());
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({
        by_reason: [{ reason: "damaged", count: 2 }],
        attention_total: 2,
      }),
    );
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 3,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    await renderPage();
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect.element(page.getByText("Files were sent for another try")).toBeInTheDocument();
    await expect
      .element(page.getByText(/Rejected files were left as they are/))
      .toBeInTheDocument();
    await expect.element(page.getByText("2 files, 1 reason")).toBeInTheDocument();
    const card = page.getByRole("region", { name: /Needs attention/ });
    await expect
      .element(card.getByText(/Files marked Needs a change were sent for another try/))
      .toBeInTheDocument();
  });

  test("does not claim a retry when the needs-change count did not fall", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 3,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    await renderPage();
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await expect.element(page.getByText(/Scan started/)).toBeInTheDocument();
    await expect.element(page.getByText("Files were sent for another try")).not.toBeInTheDocument();
  });
});

describe("ingestion blockers", () => {
  test.each([0, 1, 2, 3, 4, 5, 6, 7])(
    "shows independent conditions for combination %i without disabling scans",
    async (combination) => {
      const blockers: IngestionApi.IngestionStatus["blockers"] = [];
      if (combination & 1) blockers.push({ kind: "paused_by_admin" });
      if (combination & 2)
        blockers.push({
          kind: "dependency",
          reason: "library_full",
          since: "2026-10-16T03:00:00Z",
        });
      if (combination & 4) blockers.push({ kind: "restarting" });
      vi.mocked(getIngestionStatus).mockResolvedValue({ blockers });
      await renderPage();
      await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
      expect(page.getByRole("heading", { name: "Imports are paused" }).query() !== null).toBe(
        combination !== 0,
      );
      expect(
        page.getByText("New imports have been paused by an administrator.").query() !== null,
      ).toBe(Boolean(combination & 1));
      expect(page.getByText("The library folder has no free space left.").query() !== null).toBe(
        Boolean(combination & 2),
      );
      expect(page.getByText("The ingestion worker is restarting.").query() !== null).toBe(
        Boolean(combination & 4),
      );
      await expect
        .element(page.getByRole("button", { name: "Scan ingestion folder" }))
        .toBeEnabled();
      if (combination & 1)
        await expect
          .element(page.getByRole("link", { name: "Open Acquisition settings" }))
          .toHaveAttribute("href", "/admin/settings/acquisition");
      if (combination & 2)
        await expect
          .element(
            (await rendered(page.getByText(/Dependency problem since/))).querySelector("time"),
          )
          .toHaveAttribute("datetime", "2026-10-16T03:00:00Z");
      if (combination !== 0) {
        const notice = page.getByRole("region", { name: "Imports are paused" });
        expect((await rendered(notice)).closest('[aria-live="polite"]')).not.toBeNull();
        await expect
          .element(notice)
          .toMatchTextContent(
            "Queued files are kept. Work already started can finish, and scans and recovery continue. New imports resume when Import files is on, dependencies are available and the worker is running.",
          );
        if (combination === 7) {
          const copy = (await rendered(notice)).textContent;
          expect(copy.indexOf("administrator.")).toBeLessThan(copy.indexOf("no free space"));
          expect(copy.indexOf("no free space")).toBeLessThan(copy.indexOf("worker is restarting"));
        }
      }
    },
  );

  test.each([
    [
      "database_unavailable",
      "Reverie cannot reach the database that ingestion uses.",
      "Check that the database is running and that the ingestion database account can sign in.",
    ],
    [
      "ingestion_folder_unavailable",
      "Reverie cannot read the ingestion folder.",
      "Check that the folder exists, is mounted, and can be read by the account Reverie runs as.",
    ],
    [
      "library_folder_unavailable",
      "Reverie cannot write to the library folder.",
      "Check that the library folder is mounted and can be written by the account Reverie runs as.",
    ],
    [
      "library_full",
      "The library folder has no free space left.",
      "Free some space on the library's disk.",
    ],
    [
      "another_instance",
      "Another Reverie server is already running ingestion for this library.",
      "Make sure only one Reverie server uses this database.",
    ],
    [
      "unspecified",
      "Reverie hit a problem it cannot describe here.",
      "Check the server log for the cause.",
    ],
  ] satisfies [IngestionApi.DependencyReason, string, string][])(
    "keeps the approved guidance for %s",
    async (reason, sentence, check) => {
      vi.mocked(getIngestionStatus).mockResolvedValue({
        blockers: [{ kind: "dependency", reason, since: "2026-10-16T03:00:00Z" }],
      });
      await renderPage();
      await expect.element(page.getByText(sentence)).toBeInTheDocument();
      await expect.element(page.getByText(check, { exact: false })).toBeInTheDocument();
    },
  );

  test("keeps scan feedback independent when a restart follows an accepted scan", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 2,
      deferred: 1,
      suppressed: 0,
      monitor: "/m",
    });
    const client = await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    const title = "Scan started. 2 queued, 1 waiting, 0 unchanged.";
    await expect.element(page.getByText(title)).toBeInTheDocument();
    {
      client.setQueryData(queryKeys.ingestion.status(), {
        blockers: [{ kind: "paused_by_admin" }, { kind: "restarting" }],
      });
    }
    await expect.element(page.getByText(title)).toBeInTheDocument();
    await expect.element(page.getByText("The ingestion worker is restarting.")).toBeInTheDocument();
    await expect
      .element(page.getByText(/The scan queued new or changed files/))
      .toBeInTheDocument();
    await expect.element(page.getByText(/being imported now/)).not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await vi.waitFor(() => {
      expect(scanIngestion).toHaveBeenCalledTimes(2);
    });
  });

  test("suppresses the running line during a pause and restores it after recovery", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [{ kind: "restarting" }] });
    const client = await renderPage();
    await expect.element(page.getByText("The ingestion worker is restarting.")).toBeInTheDocument();
    await expect.element(page.getByText(/Latest activity\./)).not.toBeInTheDocument();
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [] });
    {
      await client.invalidateQueries({ queryKey: queryKeys.ingestion.status() });
    }
    await expect.element(page.getByText(/Latest activity\./)).toBeInTheDocument();
    await expect
      .element(page.getByRole("heading", { name: "Imports are paused" }))
      .not.toBeInTheDocument();
  });

  test("a failed refresh replaces stale blockers with the quiet failure and retry recovers", async () => {
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [{ kind: "paused_by_admin" }] });
    const client = await renderPage();
    await expect
      .element(page.getByText("New imports have been paused by an administrator."))
      .toBeInTheDocument();
    vi.mocked(getIngestionStatus).mockRejectedValue(new TypeError("network unavailable"));
    {
      await client.invalidateQueries({ queryKey: queryKeys.ingestion.status() });
    }
    await expect
      .element(page.getByText("Could not check whether ingestion is running."))
      .toBeInTheDocument();
    await expect.element(page.getByText("Files may still be importing.")).toBeInTheDocument();
    await expect
      .element(page.getByRole("heading", { name: "Imports are paused" }))
      .not.toBeInTheDocument();
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [] });
    await userEvent.click(
      page.getByRole("status", { name: "Ingestion status" }).getByRole("button", {
        name: "Try again",
      }),
    );
    await vi.waitFor(async () => {
      await expect.element(page.getByText("Files may still be importing.")).not.toBeInTheDocument();
    });
  });

  test.each([
    [401, "Your session has ended", "Sign in again to see ingestion status."],
    [403, "This account cannot see ingestion status", "Sign in as an administrator."],
  ])("status %i uses the permission treatment", async (code, title, detail) => {
    vi.mocked(getIngestionStatus).mockRejectedValue(
      new ApiError(code, null, "denied", "/private/path"),
    );
    await renderPage();
    await expect.element(page.getByRole("alert")).toMatchTextContent(title);
    await expect.element(page.getByText(detail)).toBeInTheDocument();
    await expect.element(page.getByText("/private/path")).not.toBeInTheDocument();
  });

  test("shows no notice while the initial probe observation is loading", async () => {
    vi.mocked(getIngestionStatus).mockReturnValue(new Promise(() => {}));
    await renderPage();
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
    await expect
      .element(page.getByRole("heading", { name: "Imports are paused" }))
      .not.toBeInTheDocument();
    await expect
      .element(page.getByText(/checking ingestion dependencies/i))
      .not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
  });
});

describe("Needs attention", () => {
  test("says nothing about files being sent for another try before any scan", async () => {
    await renderPage();
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
    await expect.element(page.getByText(/were sent for another try/)).not.toBeInTheDocument();
  });

  test("headline total equals the sum of the group counts and excludes ignored files", async () => {
    await renderPage();
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
    const card = page.getByRole("region", { name: /Needs attention/ });
    const shown = Array.from((await rendered(card)).querySelectorAll("button[aria-expanded]")).map(
      (b) => {
        const label = /(\d+) files?$/.exec(b.textContent)?.[1];
        return { ignored: b.textContent.includes("Ignored"), count: Number(label) };
      },
    );
    expect(shown).toHaveLength(6);
    expect(shown.find((g) => g.ignored)?.count).toBe(14);
    const attentionSum = shown.filter((g) => !g.ignored).reduce((sum, g) => sum + g.count, 0);
    expect(attentionSum).toBe(7);
  });

  test("orders groups by severity and gives each its action sentence once", async () => {
    await renderPage();
    const card = page.getByRole("region", { name: /Needs attention/ });
    await expect.element(card.getByText("The file is damaged")).toBeInTheDocument();
    const titles = Array.from(
      (await rendered(card)).querySelectorAll("button[aria-expanded] strong"),
    ).map((el) => el.textContent);
    expect(titles).toEqual([
      "The file is damaged",
      "Not a valid EPUB",
      "Reason not specified",
      "Needs a change",
      "Failed after several tries",
      "Ignored: format not accepted",
    ]);
    await expect
      .poll(() => page.getByText("Download or export the book again, then scan.").elements())
      .toHaveLength(1);
  });

  test("expands the first group on load and lists a two-reason file once with an Also line", async () => {
    await renderPage();
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .toBeInTheDocument();
    await expect.element(page.getByText("Also: Not a valid EPUB")).toBeInTheDocument();
    await expect
      .poll(() => page.getByText("Marlow - The Salt Archive.epub", { exact: false }).elements())
      .toHaveLength(1);
    await expect.element(page.getByText("Showing 2 of 2")).toBeInTheDocument();
    await expect.element(page.getByText("incoming/older/")).toHaveClass("text-fg-muted");
    expect(vi.mocked(listInputs).mock.calls.map(([p]) => p.reason)).toEqual(["damaged"]);
  });

  test("separates several secondary reasons in the Also line with semicolons", async () => {
    vi.mocked(listInputs).mockResolvedValue({
      items: [
        inputItem(1, {
          path: "incoming/Marlow - The Salt Archive.epub",
          reasons: ["damaged", "invalid_structure", "unspecified"],
        }),
      ],
      next_cursor: null,
    });
    await renderPage();
    await expect
      .element(page.getByText("Also: Not a valid EPUB; Reason not specified"))
      .toBeInTheDocument();
  });

  test("collapsed groups make no requests; expanding one requests its own reason", async () => {
    await renderPage();
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .toBeInTheDocument();
    expect(listInputs).toHaveBeenCalledTimes(1);
    await userEvent.click(page.getByRole("button", { name: /Not a valid EPUB/ }));
    await expect.element(page.getByText("Book 50.epub", { exact: false })).toBeInTheDocument();
    expect(vi.mocked(listInputs).mock.calls.map(([p]) => p.reason)).toEqual([
      "damaged",
      "invalid_structure",
    ]);
  });

  test("the group header is a button that toggles aria-expanded", async () => {
    await renderPage();
    const button = await rendered(page.getByRole("button", { name: /The file is damaged/ }));
    await expect.element(button).toHaveAttribute("aria-expanded", "true");
    await expect.element(button).toHaveAttribute("aria-controls");
    await userEvent.click(button, { force: true });
    await expect.element(button).toHaveAttribute("aria-expanded", "false");
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .not.toBeInTheDocument();
  });

  test("Show more opens the next page by its own cursor and keeps the loaded rows", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      Promise.resolve(
        cursor === undefined
          ? { items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" }
          : { items: [inputItem(2), inputItem(3)], next_cursor: null },
      ),
    );
    await renderPage();
    await expect.element(page.getByText("Showing 2 of 3")).toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    await expect.element(page.getByText("Showing 3 of 3")).toBeInTheDocument();
    await expect
      .poll(() => page.getByText("Book 2.epub", { exact: false }).elements())
      .toHaveLength(1);
    await expect.element(page.getByText("Book 3.epub", { exact: false })).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Show more" })).not.toBeInTheDocument();
    expect(vi.mocked(listInputs).mock.calls.at(-1)?.[0]).toMatchObject({
      reason: "damaged",
      cursor: "cur-1",
    });
  });

  test("loading the final page moves focus to the count line instead of dropping it", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      Promise.resolve(
        cursor === undefined
          ? { items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" }
          : { items: [inputItem(3)], next_cursor: null },
      ),
    );
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    const count = page.getByText("Showing 3 of 3");
    await vi.waitFor(async () => {
      await expect.element(count).toHaveFocus();
    });
  });

  test("finishing a page does not steal focus after the user leaves its action", async () => {
    let release: (value: {
      items: ReturnType<typeof inputItem>[];
      next_cursor: null;
    }) => void = () => {};
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      cursor === undefined
        ? Promise.resolve({ items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" })
        : new Promise((resolve) => {
            release = resolve;
          }),
    );
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    const header = await rendered(page.getByRole("button", { name: /The file is damaged/ }));
    (await rendered(header)).focus();
    {
      release({ items: [inputItem(3)], next_cursor: null });
    }
    await expect.element(page.getByText("Showing 3 of 3")).toBeInTheDocument();
    await expect.element(header).toHaveFocus();
  });

  test("Show more keeps focus on its button while more pages remain", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 5 }], attention_total: 5 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      Promise.resolve(
        cursor === undefined
          ? { items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" }
          : { items: [inputItem(3), inputItem(4)], next_cursor: "cur-2" },
      ),
    );
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    await expect.element(page.getByText("Showing 4 of 5")).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Show more" })).toHaveFocus();
  });

  test("a failed later page moves focus to its Try again, and a successful retry to the count line", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      cursor === undefined
        ? Promise.resolve({ items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" })
        : Promise.reject(new ApiError(500, null, "Internal", "")),
    );
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    await expect.element(page.getByText("Could not load these files.")).toBeInTheDocument();
    const card = page.getByRole("region", { name: /Needs attention/ });
    const retry = await rendered(card.getByRole("button", { name: "Try again" }));
    await vi.waitFor(async () => {
      await expect.element(retry).toHaveFocus();
    });
    vi.mocked(listInputs).mockResolvedValue({ items: [inputItem(3)], next_cursor: null });
    await userEvent.click(retry, { force: true });
    const count = page.getByText("Showing 3 of 3");
    await vi.waitFor(async () => {
      await expect.element(count).toHaveFocus();
    });
  });

  test("Show more stays in the tab order and announces a busy state while loading", async () => {
    let resolveMore: (v: {
      items: ReturnType<typeof inputItem>[];
      next_cursor: null;
    }) => void = () => {};
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      cursor === undefined
        ? Promise.resolve({ items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" })
        : new Promise((resolve) => {
            resolveMore = resolve;
          }),
    );
    await renderPage();
    const more = await rendered(page.getByRole("button", { name: "Show more" }));
    await userEvent.click(more);
    const loading = await rendered(page.getByRole("button", { name: "Loading more" }));
    await expect.element(loading).toHaveAttribute("data-pending");
    await expect.element(loading).toHaveFocus();
    await userEvent.keyboard("{Enter} ");
    await userEvent.click(loading, { force: true });
    expect(listInputs).toHaveBeenCalledTimes(2);
    {
      resolveMore({ items: [inputItem(3)], next_cursor: null });
    }
    await expect.element(page.getByText("Loaded 1 more file")).toBeInTheDocument();
  });

  test("an expanded group refetches its first page on window focus", async () => {
    await renderPage();
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .toBeInTheDocument();
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(77, { path: "incoming/Fresh Arrival.epub" })],
      next_cursor: null,
    });
    {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    }
    await expect
      .element(page.getByText("Fresh Arrival.epub", { exact: false }))
      .toBeInTheDocument();
  });

  test("a second press on a pending scan sends no second request", async () => {
    vi.mocked(scanIngestion).mockReturnValue(new Promise(() => {}));
    await renderPage();
    const button = await rendered(page.getByRole("button", { name: "Scan ingestion folder" }));
    await userEvent.click(button, { force: true });
    const pending = await rendered(page.getByRole("button", { name: "Scanning" }));
    await expect.element(pending).toHaveAttribute("data-pending");
    await expect.element(pending).toHaveFocus();
    await userEvent.click(pending, { force: true });
    await userEvent.keyboard("{Enter}");
    await userEvent.keyboard(" ");
    expect(scanIngestion).toHaveBeenCalledTimes(1);
  });

  test("a failed later page keeps loaded rows and offers Try again", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      cursor === undefined
        ? Promise.resolve({ items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" })
        : Promise.reject(new ApiError(500, null, "Internal", "")),
    );
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    await expect.element(page.getByText("Could not load these files.")).toBeInTheDocument();
    await expect.element(page.getByText("Book 1.epub", { exact: false })).toBeInTheDocument();
    vi.mocked(listInputs).mockResolvedValue({ items: [inputItem(3)], next_cursor: null });
    const card = page.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(card.getByRole("button", { name: "Try again" }));
    await expect.element(page.getByText("Book 3.epub", { exact: false })).toBeInTheDocument();
  });

  test("a failed first page shows the group error with a retry", async () => {
    vi.mocked(listInputs).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    await renderPage();
    await expect.element(page.getByText("Could not load these files.")).toBeInTheDocument();
    const card = page.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(card.getByRole("button", { name: "Try again" }));
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .toBeInTheDocument();
  });

  test("retrying a failed first page keeps keyboard focus on the group header", async () => {
    vi.mocked(listInputs).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    await renderPage();
    await expect.element(page.getByText("Could not load these files.")).toBeInTheDocument();
    const card = page.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(card.getByRole("button", { name: "Try again" }));
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .toBeInTheDocument();
    await expect.element(card.getByRole("button", { name: /The file is damaged/ })).toHaveFocus();
  });

  test("failed retry focuses only its own group and Tab can leave a group", async () => {
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      reason === "invalid_structure"
        ? Promise.reject(new ApiError(500, null, "Internal", ""))
        : Promise.resolve(DAMAGED_PAGE),
    );
    await renderPage();
    const user = userEvent;
    await expect
      .element(page.getByText("Marlow - The Salt Archive.epub", { exact: false }))
      .toBeInTheDocument();
    const first = await rendered(page.getByRole("button", { name: /The file is damaged/ }));
    const second = await rendered(
      page.getByRole("button", { name: /Not a valid EPUB.*Try another copy/ }),
    );
    await user.click(second);
    const panel = page.getByRole("group", { name: /Not a valid EPUB.*Try another copy/ });
    const retry = await rendered(panel.getByRole("button", { name: "Try again" }));
    vi.mocked(listInputs).mockResolvedValue({ items: [inputItem(50)], next_cursor: null });
    (await rendered(retry)).focus();
    await user.keyboard("{Enter}");
    await expect.element(panel.getByText("Book 50.epub", { exact: false })).toBeInTheDocument();
    await expect.element(second).toHaveFocus();
    await expect.element(first).not.toHaveFocus();
    const last = await rendered(page.getByRole("button", { name: /Ignored: format not accepted/ }));
    const lastGroup = document.getElementById(
      (await rendered(last)).getAttribute("aria-controls") ?? "",
    )?.parentElement;
    if (lastGroup === undefined || lastGroup === null) throw new Error("group missing");
    (await rendered(last)).focus();
    await user.tab();
    expect(lastGroup.contains(document.activeElement)).toBe(false);
    await user.tab({ shift: true });
    await expect.element(last).toHaveFocus();
  });

  test("count refresh retains the user's group expansion", async () => {
    const client = await renderPage();
    const user = userEvent;
    const first = await rendered(page.getByRole("button", { name: /The file is damaged/ }));
    const second = await rendered(
      page.getByRole("button", { name: /Not a valid EPUB.*Try another copy/ }),
    );
    await user.click(first);
    await user.click(second);
    await expect.element(page.getByText("Book 50.epub", { exact: false })).toBeInTheDocument();
    {
      client.setQueryData(queryKeys.ingestion.counts(), countsFixture());
    }
    await expect.element(first).toHaveAttribute("aria-expanded", "false");
    await expect.element(second).toHaveAttribute("aria-expanded", "true");
    expect(vi.mocked(listInputs).mock.calls.map(([p]) => p.reason)).toEqual([
      "damaged",
      "invalid_structure",
    ]);
  });

  test("the ignored group is muted, counted on its own and requests format_not_accepted", async () => {
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      Promise.resolve(
        reason === "format_not_accepted"
          ? {
              items: [
                inputItem(9, {
                  path: "incoming/cover.jpg",
                  status: "not_accepted",
                  outcome: null,
                  primary_reason: "format_not_accepted",
                  reasons: ["format_not_accepted"],
                  completed_at: null,
                }),
              ],
              next_cursor: null,
            }
          : DAMAGED_PAGE,
      ),
    );
    await renderPage();
    await expect.element(page.getByText("Showing 2 of 2")).toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: /Ignored: format not accepted/ }));
    await expect.element(page.getByText("cover.jpg", { exact: false })).toBeInTheDocument();
    expect(vi.mocked(listInputs).mock.calls.at(-1)?.[0]).toMatchObject({
      reason: "format_not_accepted",
    });
    await expect.element(page.getByText("14 files")).toBeInTheDocument();
  });

  test("a legacy row shows the masked class and never raw text", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "unspecified", count: 1 }], attention_total: 1 }),
    );
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(4, { primary_reason: "unspecified", reasons: ["unspecified"] })],
      next_cursor: null,
    });
    await renderPage();
    await expect
      .element(
        page
          .getByRole("region", { name: /Needs attention/ })
          .getByRole("button", { name: /Reason not specified/ }),
      )
      .toBeInTheDocument();
    await expect
      .element(page.getByText("Replace the file, or check the server log, then scan."))
      .toBeInTheDocument();
  });

  test("a non-UTF-8 file name renders with its escapes and does not break the row", async () => {
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(5, { path: "incoming/Book \\xe9\\xff <b>.epub" })],
      next_cursor: null,
    });
    await renderPage();
    await expect
      .element(page.getByText("Book \\xe9\\xff <b>.epub", { exact: false }))
      .toBeInTheDocument();
  });

  test("a file never tried shows no time", async () => {
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(6, { completed_at: null })],
      next_cursor: null,
    });
    await renderPage();
    await expect.element(page.getByText("Book 6.epub", { exact: false })).toBeInTheDocument();
    const card = page.getByRole("region", { name: /Needs attention/ });
    await expect.poll(() => card.getByText(/\d{1,2}:\d{2}/).elements()).toHaveLength(0);
  });

  test("shows the empty message when no class needs attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue({ by_reason: [], attention_total: 0 });
    await renderPage();
    await expect.element(page.getByText("Nothing needs attention.")).toBeInTheDocument();
    await expect
      .element(
        page.getByText(
          "Every file Reverie has seen was imported, is waiting its turn, or was ignored.",
        ),
      )
      .toBeInTheDocument();
    expect(listInputs).not.toHaveBeenCalled();
  });

  test("shows a busy skeleton with status text while counts load", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    await renderPage();
    await expect.element(page.getByText("Loading files that need attention")).toBeInTheDocument();
    await expect
      .element(page.getByRole("region", { name: /Needs attention/ }))
      .toHaveAttribute("aria-busy", "true");
  });

  test("the group chevron opts out of motion under reduced motion", async () => {
    await renderPage();
    const chevron = (
      await rendered(page.getByRole("button", { name: /The file is damaged/ }))
    ).querySelector("svg");
    await expect.element(chevron).toHaveClass("motion-reduce:transition-none");
  });

  test("the needs-attention skeletons opt out of motion under reduced motion", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    await renderPage();
    const region = page.getByRole("region", { name: /Needs attention/ });
    const skeletons = (await rendered(region)).querySelectorAll<HTMLElement>(
      "[data-slot=skeleton]",
    );
    expect(skeletons.length).toBeGreaterThan(0);
    for (const skeleton of skeletons) {
      await expect.element(skeleton).toHaveClass("motion-reduce:animate-none");
    }
  });

  test("shows one error with a retry when the counts fail", async () => {
    vi.mocked(getInputCounts).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    await renderPage();
    const region = page.getByRole("region", { name: /Needs attention/ });
    await expect.element(region.getByText("Could not load this list.")).toBeInTheDocument();
    await expect
      .element(region.getByText("Your files are untouched. Try again."))
      .toBeInTheDocument();
    await userEvent.click(
      page.getByRole("region", { name: /Needs attention/ }).getByRole("button", {
        name: "Try again",
      }),
    );
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
  });

  test("keeps the existing metric cards", async () => {
    await renderPage();
    await expect.element(page.getByText("1,204")).toBeInTheDocument();
    await expect.element(page.getByText("Recent ingestion batches")).toBeInTheDocument();
  });

  test("a non-admin is redirected and nothing is requested", async () => {
    await renderPage(ADULT_ME);
    await expect.element(page.getByTestId("library-page")).toBeInTheDocument();
    expect(getInputCounts).not.toHaveBeenCalled();
    expect(getIngestionStatus).not.toHaveBeenCalled();
    expect(getEnrichmentFailureCounts).not.toHaveBeenCalled();
  });
});

describe("Enrichment problems", () => {
  test("groups by source and class, once per book, and opens the first group", async () => {
    vi.mocked(listEnrichmentFailures).mockResolvedValue({
      items: [
        failureItem(1),
        failureItem(2, {
          status: "skipped",
          attempt_count: 1,
          also: [{ source: "hardcover", class: "not_found" }],
        }),
      ],
      next_cursor: null,
    });
    await renderPage();
    const card = page.getByRole("region", { name: /Enrichment problems/ });
    await expect.element(card.getByRole("heading")).toMatchTextContent("4 books, 3 reasons");
    const titles = Array.from(
      (await rendered(card)).querySelectorAll("button[aria-expanded] strong"),
    ).map((e) => e.textContent);
    expect(titles).toEqual([
      "Hardcover: no record found",
      "Open Library: timed out",
      "Reason not specified",
    ]);
    await expect.element(card.getByText("Book title 1")).toBeInTheDocument();
    await expect.element(card.getByText("Will retry, tried 3 times")).toBeInTheDocument();
    await expect.element(card.getByText("Stopped retrying, tried 1 time")).toBeInTheDocument();
    await expect.element(card.getByText("Also: Hardcover, no record found")).toBeInTheDocument();
    expect(vi.mocked(listEnrichmentFailures).mock.calls[0]?.[0]).toMatchObject({
      source: "hardcover",
      class: "not_found",
    });
  });

  test("never displays the attempt time, which can be a future retry anchor", async () => {
    await renderPage();
    await expect.element(page.getByText("Book title 1")).toBeInTheDocument();
    await expect.element(page.getByText(/2099|Jan/)).not.toBeInTheDocument();
    const card = page.getByRole("region", { name: /Enrichment problems/ });
    await expect.poll(() => card.getByText(/\d{1,2}:\d{2}/).elements()).toHaveLength(0);
  });

  test("a group with no source requests its class alone", async () => {
    await renderPage();
    const card = page.getByRole("region", { name: /Enrichment problems/ });
    await userEvent.click(card.getByRole("button", { name: /Reason not specified/ }));
    await expect.element(card.getByText("Book title 1").first()).toBeInTheDocument();
    const call = vi.mocked(listEnrichmentFailures).mock.calls.find(([p]) => p.source === null);
    expect(call?.[0]).toMatchObject({ source: null, class: "unspecified" });
  });

  test("Try again walks through queuing and queued without dropping the row", async () => {
    let finish: () => void = () => {};
    vi.mocked(triggerEnrichment).mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    const pending = await rendered(
      page.elementLocator(row).getByRole("button", { name: "Queuing" }),
    );
    await expect.element(pending).toHaveAttribute("data-pending");
    await expect.element(pending).toHaveFocus();
    await userEvent.keyboard("{Enter} ");
    await userEvent.click(pending, { force: true });
    expect(triggerEnrichment).toHaveBeenCalledOnce();
    {
      finish();
    }
    await expect
      .element(page.elementLocator(row).getByText("Queued. It leaves this list once it runs."))
      .toBeInTheDocument();
    await expect.element(page.getByText("Book title 1")).toBeInTheDocument();
    expect(triggerEnrichment).toHaveBeenCalledWith(failureItem(1).manifestation_id);
  });

  test("a retried book that fails again shows Try again once its row carries a newer attempt", async () => {
    vi.mocked(triggerEnrichment).mockResolvedValue();
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    await expect
      .element(page.elementLocator(row).getByText("Queued. It leaves this list once it runs."))
      .toBeInTheDocument();
    vi.mocked(listEnrichmentFailures).mockResolvedValue({
      items: [
        failureItem(1, { attempted_at: "2099-01-02T00:00:00Z", attempt_count: 1 }),
        failureItem(2),
      ],
      next_cursor: null,
    });
    {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    }
    await expect
      .element(page.elementLocator(row).getByRole("button", { name: "Try again" }))
      .toBeInTheDocument();
    await expect.element(page.elementLocator(row).getByText(/Queued\./)).not.toBeInTheDocument();
  });

  test("a refetch that returns the same attempt keeps the Queued confirmation", async () => {
    vi.mocked(triggerEnrichment).mockResolvedValue();
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    await expect
      .element(page.elementLocator(row).getByText("Queued. It leaves this list once it runs."))
      .toBeInTheDocument();
    const calls = vi.mocked(listEnrichmentFailures).mock.calls.length;
    {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    }
    await vi.waitFor(() => {
      expect(vi.mocked(listEnrichmentFailures).mock.calls.length).toBeGreaterThan(calls);
    });
    await expect
      .element(page.elementLocator(row).getByText("Queued. It leaves this list once it runs."))
      .toBeInTheDocument();
  });

  test("a successful retry moves focus to the Queued confirmation", async () => {
    vi.mocked(triggerEnrichment).mockResolvedValue();
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    const queued = page.elementLocator(row).getByText("Queued. It leaves this list once it runs.");
    await vi.waitFor(async () => {
      await expect.element(queued).toHaveFocus();
    });
  });

  test("a queued receipt does not steal focus after the user leaves retry", async () => {
    let release: () => void = () => {};
    vi.mocked(triggerEnrichment).mockReturnValue(
      new Promise((resolve) => {
        release = resolve;
      }),
    );
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    const other = await rendered(page.getByRole("button", { name: /The file is damaged/ }));
    (await rendered(other)).focus();
    {
      release();
    }
    await expect
      .element(page.elementLocator(row).getByText("Queued. It leaves this list once it runs."))
      .toBeInTheDocument();
    await expect.element(other).toHaveFocus();
  });

  test("a second press on a queuing retry sends no second request", async () => {
    vi.mocked(triggerEnrichment).mockReturnValue(new Promise(() => {}));
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    const queuing = await rendered(
      page.elementLocator(row).getByRole("button", { name: "Queuing" }),
    );
    (await rendered(queuing)).focus();
    await userEvent.keyboard("{Enter}");
    expect(triggerEnrichment).toHaveBeenCalledTimes(1);
  });

  test("a failed retry says so and offers Try again on the same row", async () => {
    vi.mocked(triggerEnrichment).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    await renderPage();
    const row = (await rendered(page.getByText("Book title 1"))).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(page.elementLocator(row).getByRole("button", { name: "Try again" }));
    await expect
      .element(page.elementLocator(row).getByText("Could not queue it."))
      .toBeInTheDocument();
    await expect
      .element(page.elementLocator(row).getByRole("button", { name: "Try again" }))
      .toBeInTheDocument();
  });

  test("shows the empty, loading and error states", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockResolvedValueOnce({ by_failure: [], total: 0 });
    await renderPage();
    await expect.element(page.getByText("No enrichment problems.")).toBeInTheDocument();
    await expect
      .element(page.getByText("Every book was enriched, or is waiting its turn."))
      .toBeInTheDocument();
  });

  test("shows a busy skeleton while loading", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockReturnValue(new Promise(() => {}));
    await renderPage();
    await expect
      .element(page.getByText("Loading books with enrichment problems"))
      .toBeInTheDocument();
  });

  test("the enrichment skeletons opt out of motion under reduced motion", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockReturnValue(new Promise(() => {}));
    await renderPage();
    const status = page.getByText("Loading books with enrichment problems");
    const skeletons =
      (await rendered(status)).parentElement?.querySelectorAll<HTMLElement>(
        "[data-slot=skeleton]",
      ) ?? [];
    expect(skeletons.length).toBeGreaterThan(0);
    for (const skeleton of skeletons) {
      await expect.element(skeleton).toHaveClass("motion-reduce:animate-none");
    }
  });

  test("shows a retryable error when the counts fail", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockRejectedValueOnce(
      new ApiError(500, null, "Internal", ""),
    );
    await renderPage();
    const card = page.getByRole("region", { name: /Enrichment problems/ });
    await expect
      .element(card.getByText("Your books are untouched. Try again."))
      .toBeInTheDocument();
    await userEvent.click(card.getByRole("button", { name: "Try again" }));
    await expect.element(card.getByRole("heading")).toMatchTextContent("4 books, 3 reasons");
  });
});

describe("polling", () => {
  beforeEach(() => {
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    });
  });

  test("a paused notice does not stop polling admitted work or expanded groups", async () => {
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [{ kind: "paused_by_admin" }] });
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    await renderPage();
    await flush();
    await expect.element(page.getByText(/Latest activity\./)).not.toBeInTheDocument();
    const before = {
      counts: vi.mocked(getInputCounts).mock.calls.length,
      activity: vi.mocked(getDashboardActivity).mock.calls.length,
      inputs: vi.mocked(listInputs).mock.calls.length,
    };
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    }
    expect(vi.mocked(getInputCounts).mock.calls.length - before.counts).toBe(2);
    expect(vi.mocked(getDashboardActivity).mock.calls.length - before.activity).toBe(2);
    expect(vi.mocked(listInputs).mock.calls.length - before.inputs).toBe(2);
  });

  test("status polls once every 30 seconds through pause and restart, suspends hidden reads and refetches on focus", async () => {
    vi.mocked(getIngestionStatus).mockResolvedValue({
      blockers: [{ kind: "paused_by_admin" }, { kind: "restarting" }],
    });
    await renderPage();
    await flush();
    expect(getIngestionStatus).toHaveBeenCalledTimes(1);
    {
      await vi.advanceTimersByTimeAsync(STATUS_POLL_INTERVAL_MS - 40);
    }
    expect(getIngestionStatus).toHaveBeenCalledTimes(1);
    {
      await vi.advanceTimersByTimeAsync(40);
    }
    expect(getIngestionStatus).toHaveBeenCalledTimes(2);
    {
      focusManager.setFocused(false);
    }
    {
      await vi.advanceTimersByTimeAsync(STATUS_POLL_INTERVAL_MS * 2);
    }
    expect(getIngestionStatus).toHaveBeenCalledTimes(2);
    {
      focusManager.setFocused(true);
    }
    await flush();
    expect(getIngestionStatus).toHaveBeenCalledTimes(3);
  });

  test("status always refetches on mount even when a cached observation is fresh", async () => {
    const client = await renderPage();
    await flush();
    await cleanup();
    client.setQueryDefaults(queryKeys.ingestion.status(), { staleTime: Infinity });
    client.setQueryData(queryKeys.ingestion.status(), { blockers: [{ kind: "restarting" }] });
    await renderPage(ADMIN_ME, client);
    await flush();
    expect(client.getQueryData(queryKeys.ingestion.status())).toEqual({ blockers: [] });
    expect(getIngestionStatus).toHaveBeenCalledTimes(2);
  });

  test("polls counts, activity and expanded groups every tick while a batch is running, one request each", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    await renderPage();
    await flush();
    await expect.element(page.getByText(/Latest activity\./)).toBeInTheDocument();
    await expect.element(page.getByText(/Updating every few seconds/)).toBeInTheDocument();
    const before = {
      counts: vi.mocked(getInputCounts).mock.calls.length,
      activity: vi.mocked(getDashboardActivity).mock.calls.length,
      damaged: vi.mocked(listInputs).mock.calls.length,
    };
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    }
    expect(vi.mocked(getInputCounts).mock.calls.length - before.counts).toBe(3);
    expect(vi.mocked(getDashboardActivity).mock.calls.length - before.activity).toBe(3);
    expect(vi.mocked(listInputs).mock.calls.length - before.damaged).toBe(3);
    expect(new Set(vi.mocked(listInputs).mock.calls.map(([p]) => p.reason))).toEqual(
      new Set(["damaged"]),
    );
  });

  test("lists skipped files in the running line only above zero", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({ ended_at: null, in_progress: 4, skipped: 2 }),
    );
    await renderPage();
    await flush();
    await expect.element(page.getByText(/2 skipped, 4 in progress/)).toBeInTheDocument();
  });

  test("does not poll when the latest batch has ended", async () => {
    await renderPage();
    await flush();
    const counts = vi.mocked(getInputCounts).mock.calls.length;
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 10);
    }
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(counts);
  });

  test("announces that the latest activity finished once a running batch ends", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValueOnce(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    await renderPage();
    await flush();
    await expect.element(page.getByText("Latest activity has finished")).not.toBeInTheDocument();
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    }
    await expect.element(page.getByText("Latest activity has finished")).toBeInTheDocument();
    await expect
      .element(
        page.getByText(
          /12 files in the latest batch: 11 imported, 1 did not import\. Batches are not tied/,
        ),
      )
      .toBeInTheDocument();
    await expect.element(page.getByText(/Updating every few seconds/)).not.toBeInTheDocument();
  });

  test("clears the waiting notice once a newer batch has ended, though no poll saw it running", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 8,
      suppressed: 0,
      monitor: "/m",
    });
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    await expect.element(page.getByText("Nothing is ready yet")).toBeInTheDocument();
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({
        batch_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
        total: 1,
        completed: 1,
        failed: 0,
      }),
    );
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    }
    await expect.element(page.getByText("Nothing is ready yet")).not.toBeInTheDocument();
    await expect.element(page.getByText("Latest activity has finished")).toBeInTheDocument();
  });

  test("keeps the waiting notice while the latest batch is the one seen before the scan", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 8,
      suppressed: 0,
      monitor: "/m",
    });
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    }
    await expect.element(page.getByText("Nothing is ready yet")).toBeInTheDocument();
    await expect.element(page.getByText("Latest activity has finished")).not.toBeInTheDocument();
  });

  test("a 403 during a running batch outlives the batch and keeps the scan disabled", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValueOnce(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(403, null, "Forbidden", ""));
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    await expect.element(page.getByText("This account cannot start a scan")).toBeInTheDocument();
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    }
    await expect.element(page.getByText("Latest activity has finished")).toBeInTheDocument();
    await expect.element(page.getByText("This account cannot start a scan")).toBeInTheDocument();
    await expect
      .element(page.getByRole("button", { name: "Scan ingestion folder" }))
      .toBeDisabled();
  });

  test("a failed scan alert outlives a batch that finishes", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValueOnce(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    }
    await expect.element(page.getByText("Latest activity has finished")).toBeInTheDocument();
    await expect.element(page.getByText("The scan did not start")).toBeInTheDocument();
  });

  test("polls for the two-minute window after a scan that queued files, then stops", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 2,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    const afterScan = vi.mocked(getInputCounts).mock.calls.length;
    {
      await vi.advanceTimersByTimeAsync(GRACE_WINDOW_MS - POLL_INTERVAL_MS);
    }
    expect(vi.mocked(getInputCounts).mock.calls.length - afterScan).toBeGreaterThanOrEqual(30);
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    }
    const stopped = vi.mocked(getInputCounts).mock.calls.length;
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 10);
    }
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(stopped);
  });

  test("refetches activity and counts on window focus once polling has stopped", async () => {
    await renderPage();
    await flush();
    await expect.element(page.getByText("7 files, 5 reasons")).toBeInTheDocument();
    const counts = vi.mocked(getInputCounts).mock.calls.length;
    const activity = vi.mocked(getDashboardActivity).mock.calls.length;
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 9 }], attention_total: 9 }),
    );
    {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
      await vi.advanceTimersByTimeAsync(20);
    }
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(counts + 1);
    expect(vi.mocked(getDashboardActivity).mock.calls).toHaveLength(activity + 1);
    await expect.element(page.getByText("9 files, 1 reason")).toBeInTheDocument();
  });

  test("a scan that queued and deferred nothing starts no poll", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 0,
      suppressed: 9,
      monitor: "/m",
    });
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    const afterScan = vi.mocked(getInputCounts).mock.calls.length;
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 10);
    }
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(afterScan);
  });

  test("the enrichment lists are not polled", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    await renderPage();
    await flush();
    const enrichment = vi.mocked(getEnrichmentFailureCounts).mock.calls.length;
    const lists = vi.mocked(listEnrichmentFailures).mock.calls.length;
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 5);
    }
    expect(vi.mocked(getEnrichmentFailureCounts).mock.calls).toHaveLength(enrichment);
    expect(vi.mocked(listEnrichmentFailures).mock.calls).toHaveLength(lists);
  });

  test("polling does not refetch later pages", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 3 }], attention_total: 3 }),
    );
    vi.mocked(listInputs).mockImplementation(({ cursor }) =>
      Promise.resolve(
        cursor === undefined
          ? { items: [inputItem(1), inputItem(2)], next_cursor: "cur-1" }
          : { items: [inputItem(3)], next_cursor: null },
      ),
    );
    await renderPage();
    await flush();
    await userEvent.click(page.getByRole("button", { name: "Show more" }));
    await flush();
    const laterBefore = vi
      .mocked(listInputs)
      .mock.calls.filter(([p]) => p.cursor !== undefined).length;
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 4);
    }
    expect(vi.mocked(listInputs).mock.calls.filter(([p]) => p.cursor !== undefined)).toHaveLength(
      laterBefore,
    );
  });
});

describe("bulk retry group controls", () => {
  test("bulk retry only exposes eligible input actions as siblings and sends one pending command", async () => {
    vi.mocked(retryInputs).mockReturnValue(new Promise(() => {}));
    const client = await renderPage();
    const card = page.getByRole("region", { name: /Needs attention/ });
    const buttons = await renderedAll(card.getByRole("button", { name: /Retry/ }));
    expect(buttons.map((button) => button.textContent)).toEqual(["Retry all 2", "Retry 1 file"]);
    const action = buttons[0];
    expect(action).toBeDefined();
    await expect
      .element(action.parentElement?.querySelector<HTMLElement>("button[aria-expanded]") ?? null)
      .toBeInTheDocument();
    await userEvent.click(action);
    await expect.element(action).toHaveFocus();
    await expect.element(action).toHaveAttribute("data-pending");
    {
      client.setQueryData(
        queryKeys.ingestion.counts(),
        countsFixture({ by_reason: [], attention_total: 0 }),
      );
    }
    await vi.waitFor(async () => {
      await expect
        .element(card.getByRole("button", { name: /Needs a change/ }))
        .toMatchTextContent("0 files");
    });
    await expect.element(action).not.toHaveAttribute("disabled");
    await userEvent.keyboard("{Enter} ");
    await userEvent.click(action, { force: true });
    await vi.waitFor(() => {
      expect(retryInputs).toHaveBeenCalledExactlyOnceWith("needs_change");
    });
    await expect
      .element(card.getByRole("button", { name: "Queuing 2" }))
      .toHaveAttribute("aria-disabled", "true");
    await expect.element(action).toHaveFocus();
    expect(listInputs).not.toHaveBeenCalledWith(
      expect.objectContaining({ reason: "needs_change" }),
      expect.anything(),
    );
  });

  test("bulk retry retains independent receipts after counts fall to zero and a scan completes", async () => {
    vi.mocked(retryInputs).mockImplementation((reason) =>
      Promise.resolve({ queued: reason === "needs_change" ? 2 : 1, remaining: 0 }),
    );
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [{ kind: "paused_by_admin" }] });
    const client = await renderPage();
    const action = await rendered(
      page
        .getByRole("region", { name: /Needs attention/ })
        .getByRole("button", { name: "Retry all 2" }),
    );
    (await rendered(action)).focus();
    await userEvent.click(action);
    await expect.element(page.getByText(/Queued 2\. Ingestion is paused/)).toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "Retry 1 file" }));
    await expect.element(page.getByText(/Queued 1\. Ingestion is paused/)).toBeInTheDocument();
    {
      client.setQueryData(
        queryKeys.ingestion.counts(),
        countsFixture({ by_reason: [], attention_total: 0 }),
      );
    }
    await expect.element(page.getByText(/Queued 2\. Ingestion is paused/)).toBeInTheDocument();
    await expect.element(page.getByText(/Queued 1\. Ingestion is paused/)).toBeInTheDocument();
    await expect.element(action).toBeDisabled();
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 0,
      suppressed: 0,
      monitor: "/api/v1/dashboard/activity",
    });
    await userEvent.click(page.getByRole("button", { name: "Scan ingestion folder" }));
    await vi.waitFor(() => {
      expect(scanIngestion).toHaveBeenCalledOnce();
    });
    await expect.element(page.getByText(/Queued 2\. Ingestion is paused/)).toBeInTheDocument();
    {
      client.setQueryData(queryKeys.ingestion.counts(), countsFixture());
    }
    await expect
      .element(
        page.getByRole("region", { name: /Needs attention/ }).getByRole("button", {
          name: "Retry all 2",
        }),
      )
      .toBeEnabled();
  });

  test("bulk retry retains canonical group order after refreshing to ignored files", async () => {
    vi.mocked(retryInputs).mockResolvedValue({ queued: 2, remaining: 0 });
    const client = await renderPage();
    const card = page.getByRole("region", { name: /Needs attention/ });
    const action = await rendered(card.getByRole("button", { name: "Retry all 2" }));
    (await rendered(action)).focus();
    await userEvent.click(action);
    await expect.element(card.getByText("Queued 2")).toBeInTheDocument();
    {
      client.setQueryData(
        queryKeys.ingestion.counts(),
        countsFixture({
          by_reason: [{ reason: "format_not_accepted", count: 1 }],
          attention_total: 0,
        }),
      );
    }
    await vi.waitFor(async () => {
      const toggles = card
        .getByRole("button")
        .elements()
        .filter((button) => button.hasAttribute("aria-expanded"));
      expect(toggles).toHaveLength(2);
      await expect.element(toggles[0]).toMatchTextContent("Needs a change");
      await expect.element(toggles[1]).toMatchTextContent("Ignored");
    });
    await expect.element(action).toBeDisabled();
  });

  test.each(["files", "books"])(
    "bulk retry %s queues the returned remainder before counts refresh",
    async (noun) => {
      vi.mocked(retryInputs)
        .mockResolvedValueOnce({ queued: 500, remaining: 1 })
        .mockReturnValueOnce(new Promise(() => {}));
      vi.mocked(retryEnrichmentFailures)
        .mockResolvedValueOnce({ queued: 500, remaining: 1 })
        .mockReturnValueOnce(new Promise(() => {}));
      vi.mocked(getInputCounts).mockResolvedValue(
        countsFixture({
          by_reason: [{ reason: "needs_change", count: 501 }],
          attention_total: 501,
        }),
      );
      vi.mocked(getEnrichmentFailureCounts).mockResolvedValue({
        total: 501,
        by_failure: [{ source: "openlibrary", class: "timeout", count: 501 }],
      });
      await renderPage();
      const card = page.getByRole("region", {
        name: noun === "files" ? /Needs attention/ : /Enrichment problems/,
      });
      await userEvent.click(card.getByRole("button", { name: "Retry all 501" }));
      await userEvent.click(card.getByRole("button", { name: "Retry the rest 1" }));
      await expect
        .element(card.getByRole("button", { name: "Queuing 1" }))
        .toHaveAttribute("aria-disabled", "true");
    },
  );

  test("bulk retry partial batches use returned membership, then retain empty feedback", async () => {
    vi.mocked(retryInputs)
      .mockResolvedValueOnce({ queued: 500, remaining: 1 })
      .mockResolvedValueOnce({ queued: 1, remaining: 0 });
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "needs_change", count: 501 }], attention_total: 501 }),
    );
    await renderPage();
    await userEvent.click(page.getByRole("button", { name: "Retry all 501" }));
    await expect.element(page.getByText("Queued 500, 1 remaining")).toBeInTheDocument();
    await userEvent.click(page.getByRole("button", { name: "Retry the rest 1" }));
    await expect.element(page.getByText("Queued 1")).toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Queued" })).toBeDisabled();
    expect(retryInputs).toHaveBeenCalledTimes(2);
  });

  test.each([401, 403, 500])(
    "bulk retry reports %i truthfully without automatic replay",
    async (status) => {
      vi.mocked(retryInputs).mockRejectedValue(new ApiError(status, null, "failed", ""));
      await renderPage();
      await userEvent.click(
        page.getByRole("region", { name: /Needs attention/ }).getByRole("button", {
          name: "Retry all 2",
        }),
      );
      let text: string;
      if (status === 401) {
        text = "Your session has ended. Sign in again to retry files.";
      } else if (status === 403) {
        text = "This account cannot retry files. Sign in as an administrator.";
      } else {
        text =
          "Could not confirm whether these files were queued. Refresh the group before trying again.";
      }
      await expect.element(page.getByText(text)).toHaveAttribute("role", "alert");
      const retry = await rendered(
        page
          .getByRole("region", { name: /Needs attention/ })
          .getByRole("button", { name: "Try again" }),
      );
      if (status === 403) {
        await userEvent.click(retry, { force: true });
        await expect.element(retry).toBeDisabled();
      }
      expect(retryInputs).toHaveBeenCalledOnce();
      await expect.element(page.getByText(/Nothing was changed/)).not.toBeInTheDocument();
    },
  );

  test("bulk retry books keeps collapsed feedback through an empty refresh and rearms on fresh failure", async () => {
    vi.mocked(retryEnrichmentFailures).mockResolvedValue({ queued: 3, remaining: 0 });
    const client = await renderPage();
    const card = page.getByRole("region", { name: /Enrichment problems/ });
    const [action] = await renderedAll(card.getByRole("button", { name: /Retry all/ }));
    const count = Number(action.textContent.replace("Retry all ", ""));
    await userEvent.click(action);
    await expect.element(card.getByText("Queued 3")).toBeInTheDocument();
    {
      client.setQueryData(queryKeys.enrichmentFailures.counts(), { total: 0, by_failure: [] });
    }
    await expect.element(card.getByText("Queued 3")).toBeInTheDocument();
    await expect.element(action).toBeDisabled();
    {
      client.setQueryData(queryKeys.enrichmentFailures.counts(), failureCountsFixture());
    }
    await expect
      .element(card.getByRole("button", { name: `Retry all ${String(count)}` }))
      .toBeInTheDocument();
    expect(retryEnrichmentFailures).toHaveBeenCalledOnce();
  });
});

describe("bulk retry continuation reset", () => {
  test.each(["files", "books"])(
    "bulk retry %s discards old continuation rows and reloads reused cursors",
    async (noun) => {
      vi.mocked(getInputCounts).mockResolvedValue(
        countsFixture({ by_reason: [{ reason: "needs_change", count: 5 }], attention_total: 5 }),
      );
      vi.mocked(getEnrichmentFailureCounts).mockResolvedValue({
        total: 5,
        by_failure: [{ source: "openlibrary", class: "timeout", count: 5 }],
      });
      const file = (id: number): IngestionApi.InputItem =>
        inputItem(id, {
          path: `incoming/continuation-${String(id)}.epub`,
          primary_reason: "needs_change",
          reasons: ["needs_change"],
        });
      const book = (id: number): EnrichmentApi.FailureItem =>
        failureItem(id, { title: `continuation-${String(id)}` });
      vi.mocked(listInputs).mockImplementation(({ cursor }) =>
        Promise.resolve({
          items: [file(cursor === undefined ? 1 : 2)],
          next_cursor: cursor === undefined ? "reused" : null,
        }),
      );
      vi.mocked(listEnrichmentFailures).mockImplementation(({ cursor }) =>
        Promise.resolve({
          items: [book(cursor === undefined ? 1 : 2)],
          next_cursor: cursor === undefined ? "reused" : null,
        }),
      );
      vi.mocked(retryInputs).mockResolvedValue({ queued: 2, remaining: 3 });
      vi.mocked(retryEnrichmentFailures).mockResolvedValue({ queued: 2, remaining: 3 });
      const client = await renderPage();
      const card = page.getByRole("region", {
        name: noun === "files" ? /Needs attention/ : /Enrichment problems/,
      });
      await userEvent.click(card.getByRole("button", { name: "Show more" }));
      await expect
        .element(
          card.getByText(noun === "files" ? "continuation-2.epub" : "continuation-2", {
            exact: false,
          }),
        )
        .toBeInTheDocument();
      const callsBefore =
        noun === "files"
          ? vi.mocked(listInputs).mock.calls.length
          : vi.mocked(listEnrichmentFailures).mock.calls.length;
      const untouchedKey =
        noun === "files"
          ? queryKeys.ingestion.groupAfter("retries_exhausted", "untouched")
          : queryKeys.enrichmentFailures.groupAfter("hardcover", "not_found", "untouched");
      const untouchedPage = { items: [], next_cursor: "still-valid" };
      client.setQueryData(untouchedKey, untouchedPage);
      const action = await rendered(card.getByRole("button", { name: "Retry all 5" }));
      (await rendered(action)).focus();
      await userEvent.click(action);
      await expect.element(card.getByText(/Queued 2, 3 remaining/)).toBeInTheDocument();
      await expect
        .element(
          card.getByText(noun === "files" ? "continuation-2.epub" : "continuation-2", {
            exact: false,
          }),
        )
        .not.toBeInTheDocument();
      await expect.element(action).toHaveFocus();
      expect(client.getQueryData(untouchedKey)).toEqual(untouchedPage);
      expect(
        noun === "files"
          ? vi.mocked(listInputs).mock.calls.length
          : vi.mocked(listEnrichmentFailures).mock.calls.length,
      ).toBe(callsBefore);
      vi.mocked(listInputs).mockResolvedValue({ items: [file(4)], next_cursor: null });
      vi.mocked(listEnrichmentFailures).mockResolvedValue({ items: [book(4)], next_cursor: null });
      {
        if (noun === "files") {
          client.setQueryData(queryKeys.ingestion.groupFirst("needs_change"), {
            items: [file(3)],
            next_cursor: "reused",
          });
          client.setQueryData(
            queryKeys.ingestion.counts(),
            countsFixture({
              by_reason: [{ reason: "needs_change", count: 3 }],
              attention_total: 3,
            }),
          );
        } else {
          client.setQueryData(queryKeys.enrichmentFailures.groupFirst("openlibrary", "timeout"), {
            items: [book(3)],
            next_cursor: "reused",
          });
          client.setQueryData(queryKeys.enrichmentFailures.counts(), {
            total: 3,
            by_failure: [{ source: "openlibrary", class: "timeout", count: 3 }],
          });
        }
      }
      await userEvent.click(card.getByRole("button", { name: "Show more" }));
      await expect
        .element(
          card.getByText(noun === "files" ? "continuation-4.epub" : "continuation-4", {
            exact: false,
          }),
        )
        .toBeInTheDocument();
      await expect
        .element(
          card.getByText(noun === "files" ? "continuation-2.epub" : "continuation-2", {
            exact: false,
          }),
        )
        .not.toBeInTheDocument();
      vi.mocked(retryInputs).mockResolvedValue({ queued: 3, remaining: 0 });
      vi.mocked(retryEnrichmentFailures).mockResolvedValue({ queued: 3, remaining: 0 });
      await userEvent.click(card.getByRole("button", { name: "Retry all 3" }));
      await expect.element(card.getByText(/Queued 3/)).toBeInTheDocument();
      {
        if (noun === "files") {
          client.setQueryData(queryKeys.ingestion.groupFirst("needs_change"), {
            items: [],
            next_cursor: null,
          });
          client.setQueryData(
            queryKeys.ingestion.counts(),
            countsFixture({ by_reason: [], attention_total: 0 }),
          );
        } else {
          client.setQueryData(queryKeys.enrichmentFailures.groupFirst("openlibrary", "timeout"), {
            items: [],
            next_cursor: null,
          });
          client.setQueryData(queryKeys.enrichmentFailures.counts(), { total: 0, by_failure: [] });
        }
      }
      await expect.element(card.getByText("Showing 0 of 0")).toBeInTheDocument();
      await expect
        .element(
          card.getByText(noun === "files" ? "continuation-4.epub" : "continuation-4", {
            exact: false,
          }),
        )
        .not.toBeInTheDocument();
    },
  );

  test.each(["files", "books"])(
    "bulk retry %s preserves loaded continuations on failure",
    async (noun) => {
      vi.mocked(getInputCounts).mockResolvedValue(
        countsFixture({ by_reason: [{ reason: "needs_change", count: 3 }], attention_total: 3 }),
      );
      vi.mocked(getEnrichmentFailureCounts).mockResolvedValue({
        total: 3,
        by_failure: [{ source: "openlibrary", class: "timeout", count: 3 }],
      });
      vi.mocked(listInputs).mockImplementation(({ cursor }) =>
        Promise.resolve({
          items: [
            inputItem(cursor === undefined ? 1 : 2, {
              path: cursor === undefined ? "incoming/first.epub" : "incoming/retained-tail.epub",
            }),
          ],
          next_cursor: cursor === undefined ? "tail" : null,
        }),
      );
      vi.mocked(listEnrichmentFailures).mockImplementation(({ cursor }) =>
        Promise.resolve({
          items: [
            failureItem(cursor === undefined ? 1 : 2, {
              title: cursor === undefined ? "first" : "retained-tail",
            }),
          ],
          next_cursor: cursor === undefined ? "tail" : null,
        }),
      );
      vi.mocked(retryInputs).mockRejectedValue(new ApiError(500, null, "failed", ""));
      vi.mocked(retryEnrichmentFailures).mockRejectedValue(new ApiError(500, null, "failed", ""));
      const client = await renderPage();
      const card = page.getByRole("region", {
        name: noun === "files" ? /Needs attention/ : /Enrichment problems/,
      });
      await userEvent.click(card.getByRole("button", { name: "Show more" }));
      await expect.element(card.getByText("Showing 2 of 3")).toBeInTheDocument();
      await userEvent.click(card.getByRole("button", { name: "Retry all 3" }));
      await expect
        .element(
          card.getByText(
            `Could not confirm whether these ${noun} were queued. Refresh the group before trying again.`,
          ),
        )
        .toBeInTheDocument();
      await expect
        .element(
          card.getByText(noun === "files" ? "retained-tail.epub" : "retained-tail", {
            exact: false,
          }),
        )
        .toBeInTheDocument();
      await expect.element(card.getByText("Showing 2 of 3")).toBeInTheDocument();
      const key =
        noun === "files"
          ? queryKeys.ingestion.groupAfter("needs_change", "tail")
          : queryKeys.enrichmentFailures.groupAfter("openlibrary", "timeout", "tail");
      expect(client.getQueryData(key)).toBeDefined();
    },
  );
});

describe("bulk retry retained states", () => {
  test.each([401, 403, 500])("bulk retry books reports %i without replay", async (status) => {
    vi.mocked(retryEnrichmentFailures).mockRejectedValue(new ApiError(status, null, "failed", ""));
    await renderPage();
    const card = page.getByRole("region", { name: /Enrichment problems/ });
    const [action] = await renderedAll(card.getByRole("button", { name: "Retry 1 book" }));
    await userEvent.click(action);
    let text: string;
    if (status === 401) {
      text = "Your session has ended. Sign in again to retry books.";
    } else if (status === 403) {
      text = "This account cannot retry books. Sign in as an administrator.";
    } else {
      text =
        "Could not confirm whether these books were queued. Refresh the group before trying again.";
    }
    await expect.element(card.getByText(text)).toHaveAttribute("role", "alert");
    expect(retryEnrichmentFailures).toHaveBeenCalledOnce();
    if (status === 403) {
      const button = await rendered(card.getByRole("button", { name: "Try again" }).first());
      await expect.element(button).toBeDisabled();
      await userEvent.click(button, { force: true });
      expect(retryEnrichmentFailures).toHaveBeenCalledOnce();
    }
  });

  test("bulk retry empty response stays visible while collapsed and does not fetch group rows", async () => {
    vi.mocked(retryInputs).mockResolvedValue({ queued: 0, remaining: 0 });
    await renderPage();
    const card = page.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(card.getByRole("button", { name: "Retry all 2" }));
    await expect
      .element(card.getByText("Nothing to retry. This group changed after the page loaded."))
      .toBeInTheDocument();
    await expect.element(card.getByRole("button", { name: "Queued" })).toBeDisabled();
    expect(listInputs).not.toHaveBeenCalledWith(
      expect.objectContaining({ reason: "needs_change" }),
      expect.anything(),
    );
  });

  test("bulk retry receipt survives normal polling to an empty card and grace expires", async () => {
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    });
    vi.mocked(retryInputs).mockResolvedValue({ queued: 2, remaining: 0 });
    await renderPage();
    await flush();
    const card = page.getByRole("region", { name: /Needs attention/ });
    const action = await rendered(card.getByRole("button", { name: "Retry all 2" }));
    (await rendered(action)).focus();
    await userEvent.click(action);
    await flush();
    const before = vi.mocked(getInputCounts).mock.calls.length;
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [], attention_total: 0 }),
    );
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    }
    expect(getInputCounts).toHaveBeenCalledTimes(before + 1);
    await expect.element(card.getByRole("status")).toHaveTextContent("Queued 2");
    await expect.element(action).toBeDisabled();
    await expect.element(action).not.toHaveFocus();
    {
      await vi.advanceTimersByTimeAsync(GRACE_WINDOW_MS + 20);
    }
    const stopped = vi.mocked(getInputCounts).mock.calls.length;
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    }
    expect(getInputCounts).toHaveBeenCalledTimes(stopped);
    await expect.element(card.getByText("Queued 2")).toBeInTheDocument();
  });
});

describe("Duplicates card", () => {
  const matched = {
    manifestation_id: "019a0000-0000-7000-8000-000000000101",
    work_id: "019a0000-0000-7000-8000-000000000102",
    title: "The Salt Archive",
  };
  const duplicateItem = (
    n: number,
    overrides: Partial<IngestionApi.InputItem> = {},
  ): IngestionApi.InputItem =>
    inputItem(n, {
      status: "duplicate",
      outcome: "duplicate",
      primary_reason: "duplicate",
      reasons: ["duplicate"],
      matched,
      ...overrides,
    });

  function mockDuplicates(count = 2): void {
    const counts = countsFixture();
    vi.mocked(getInputCounts).mockResolvedValue({
      ...counts,
      by_reason: [...counts.by_reason, { reason: "duplicate", count }],
    });
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      Promise.resolve(
        reason === "duplicate"
          ? { items: [duplicateItem(101), duplicateItem(102)], next_cursor: null }
          : DAMAGED_PAGE,
      ),
    );
  }

  async function card(settled = true): Promise<Locator> {
    if (settled) {
      await vi.waitFor(async () => {
        await expect
          .element(page.getByRole("region", { name: /Skipped as duplicates/ }))
          .not.toHaveAttribute("aria-busy", "true");
      });
    }
    return page.getByRole("region", { name: /Skipped as duplicates/ });
  }

  test("sits between Needs attention and Enrichment problems with its count, intro and columns", async () => {
    mockDuplicates();
    await renderPage();
    const region = await card();
    await expect.element(region.getByRole("columnheader", { name: "Matches" })).toBeInTheDocument();
    const headings = page
      .getByRole("heading", { level: 2 })
      .elements()
      .map((h) => h.textContent);
    const attention = headings.findIndex((h) => h.startsWith("Needs attention"));
    expect(headings[attention + 1]).toBe("Skipped as duplicates2 files");
    expect(headings[attention + 2]).toMatch(/Enrichment problems/);
    await expect.element(region.getByText("2 files")).toBeInTheDocument();
    await expect
      .element(
        region.getByText(
          /Removing duplicate source files under Settings, Acquisition applies only to duplicates found from then on\./,
        ),
      )
      .toBeInTheDocument();
    await expect
      .poll(() =>
        region
          .getByRole("columnheader")
          .elements()
          .map((h) => h.textContent),
      )
      .toEqual(["File", "Matches", "Last tried"]);
    const attentionRegion = page.getByRole("region", { name: /Needs attention/ });
    await expect.element(attentionRegion.getByText("7 files, 5 reasons")).toBeInTheDocument();
    await expect
      .element(attentionRegion.getByText(/Skipped as duplicates/))
      .not.toBeInTheDocument();
    await expect
      .poll(() =>
        attentionRegion
          .getByRole("button")
          .elements()
          .filter((button) => button.hasAttribute("aria-expanded")),
      )
      .toHaveLength(6);
  });

  test("lists each file once with its muted directory, matched title link and last tried time", async () => {
    mockDuplicates();
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      Promise.resolve(
        reason === "duplicate"
          ? { items: [duplicateItem(101), duplicateItem(101)], next_cursor: null }
          : DAMAGED_PAGE,
      ),
    );
    await renderPage();
    const region = await card();
    const link = region.getByRole("link", { name: matched.title });
    await expect.element(link).toHaveAttribute("href", `/b/${matched.manifestation_id}`);
    await expect.poll(() => region.getByRole("row").elements()).toHaveLength(2);
    await expect.element(region.getByText("incoming/")).toHaveClass("text-fg-muted");
    const cells = region.getByRole("row").nth(1).getByRole("cell").elements();
    await expect.element(cells[0]).toMatchTextContent("incoming/Book 101.epub");
    expect(cells[2]?.textContent).not.toBe("");
  });

  test("a file whose book is gone reads No matching book on record", async () => {
    mockDuplicates(1);
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      Promise.resolve(
        reason === "duplicate"
          ? { items: [duplicateItem(101, { matched: null })], next_cursor: null }
          : DAMAGED_PAGE,
      ),
    );
    await renderPage();
    const region = await card();
    await expect
      .element(region.getByText("No matching book on record."))
      .toHaveClass("text-fg-muted");
    await expect.element(region.getByRole("link")).not.toBeInTheDocument();
    await expect.element(region.getByText("1 file")).toBeInTheDocument();
  });

  test("the matched title links to the book page", async () => {
    mockDuplicates(1);
    await renderPage();
    const links = (await card()).getByRole("link", { name: matched.title }).elements();
    await userEvent.click(links[0] ?? document.body);
    await expect.element(page.getByTestId("book-page")).toBeInTheDocument();
  });

  test("an empty card keeps its head and intro and requests no list", async () => {
    await renderPage();
    const region = await card();
    await expect
      .element(region.getByText("No files were skipped as duplicates."))
      .toBeInTheDocument();
    await expect.element(region.getByText("0 files")).toBeInTheDocument();
    await expect
      .element(region.getByText(/Files that match a book already in the library/))
      .toBeInTheDocument();
    await expect
      .element(
        region.getByText(/If removal of duplicate source files is on under Settings, Acquisition/),
      )
      .toBeInTheDocument();
    expect(vi.mocked(listInputs).mock.calls.some(([p]) => p.reason === "duplicate")).toBe(false);
  });

  test("shows a busy skeleton while counts or the first page load", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    await renderPage();
    let region = await card(false);
    await expect.element(region).toHaveAttribute("aria-busy", "true");
    await expect
      .element(region.getByRole("status"))
      .toMatchTextContent("Loading skipped duplicates");
    await expect.element(region.getByText(/Files that match/)).not.toBeInTheDocument();
    await cleanup();
    mockDuplicates();
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      reason === "duplicate" ? new Promise(() => {}) : Promise.resolve(DAMAGED_PAGE),
    );
    await renderPage();
    region = await card(false);
    await vi.waitFor(() => {
      expect(vi.mocked(listInputs).mock.calls.some(([p]) => p.reason === "duplicate")).toBe(true);
    });
    await expect.element(region).toHaveAttribute("aria-busy", "true");
    await expect
      .element(region.getByRole("status"))
      .toMatchTextContent("Loading skipped duplicates");
    await expect.element(region.getByText("2 files")).not.toBeInTheDocument();
  });

  test("a failed first page shows the list error and Try again recovers", async () => {
    mockDuplicates(1);
    vi.mocked(listInputs).mockImplementation(({ reason }) =>
      reason === "duplicate"
        ? Promise.reject(new ApiError(500, null, "Internal", ""))
        : Promise.resolve(DAMAGED_PAGE),
    );
    await renderPage();
    let region = await card();
    await expect.element(region.getByText("Could not load this list.")).toBeInTheDocument();
    await expect
      .element(region.getByRole("alert"))
      .toMatchTextContent("Your files are untouched. Try again.");
    await expect.element(region.getByText("1 file")).not.toBeInTheDocument();
    const countsBefore = vi.mocked(getInputCounts).mock.calls.length;
    vi.mocked(listInputs).mockResolvedValue({ items: [duplicateItem(101)], next_cursor: null });
    await userEvent.click(region.getByRole("button", { name: "Try again" }));
    await expect.element(page.getByRole("link", { name: matched.title })).toBeInTheDocument();
    expect(getInputCounts).toHaveBeenCalledTimes(countsBefore);
    await cleanup();
    vi.mocked(getInputCounts).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    await renderPage();
    region = await card();
    await expect.element(region.getByText("Could not load this list.")).toBeInTheDocument();
    const failedCounts = vi.mocked(getInputCounts).mock.calls.length;
    await userEvent.click(region.getByRole("button", { name: "Try again" }));
    await expect.element(page.getByRole("link", { name: matched.title })).toBeInTheDocument();
    expect(getInputCounts).toHaveBeenCalledTimes(failedCounts + 1);
  });

  test("Show more requests the next page by its cursor and announces the loaded files", async () => {
    mockDuplicates();
    vi.mocked(listInputs).mockImplementation(({ reason, cursor }) =>
      Promise.resolve(
        reason === "duplicate"
          ? {
              items: [duplicateItem(cursor ? 102 : 101)],
              next_cursor: cursor ? null : "next-duplicate",
            }
          : DAMAGED_PAGE,
      ),
    );
    await renderPage();
    const region = await card();
    const more = await rendered(region.getByRole("button", { name: "Show more" }));
    await userEvent.click(more);
    await expect.element(region.getByText("Showing 2 of 2")).toHaveFocus();
    await expect.element(region.getByRole("status")).toMatchTextContent("Loaded 1 more file");
    expect(
      vi
        .mocked(listInputs)
        .mock.calls.some(([p]) => p.reason === "duplicate" && p.cursor === "next-duplicate"),
    ).toBe(true);
  });

  test("a non-UTF-8 file name renders with its escapes", async () => {
    mockDuplicates(1);
    vi.mocked(listInputs).mockResolvedValue({
      items: [duplicateItem(101, { path: "dupes/\\xffcopy.epub" })],
      next_cursor: null,
    });
    await renderPage();
    await expect
      .element((await card()).getByText("\\xffcopy.epub", { exact: false }))
      .toMatchTextContent("dupes/\\xffcopy.epub");
  });

  test("polls its first page with the counts while a batch runs and never polls later pages", async () => {
    vi.useFakeTimers();
    mockDuplicates();
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    vi.mocked(listInputs).mockImplementation(({ reason, cursor }) =>
      Promise.resolve(
        reason === "duplicate"
          ? {
              items: [duplicateItem(cursor ? 102 : 101)],
              next_cursor: cursor ? null : "next-duplicate",
            }
          : DAMAGED_PAGE,
      ),
    );
    await renderPage();
    await flush();
    await flush();
    const region = page.getByRole("region", { name: /Skipped as duplicates/ });
    await userEvent.click(region.getByRole("button", { name: "Show more" }));
    await flush();
    const firstCalls = (): number =>
      vi
        .mocked(listInputs)
        .mock.calls.filter(([p]) => p.reason === "duplicate" && p.cursor === undefined).length;
    const laterCalls = (): number =>
      vi
        .mocked(listInputs)
        .mock.calls.filter(([p]) => p.reason === "duplicate" && p.cursor !== undefined).length;
    const before = {
      first: firstCalls(),
      later: laterCalls(),
      counts: vi.mocked(getInputCounts).mock.calls.length,
    };
    {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    }
    expect(firstCalls() - before.first).toBe(3);
    expect(vi.mocked(getInputCounts).mock.calls.length - before.counts).toBe(3);
    expect(laterCalls()).toBe(before.later);
    {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    }
    await flush();
    expect(firstCalls() - before.first).toBe(4);
    expect(laterCalls()).toBe(before.later);
  });

  test("offers no Retry all", async () => {
    mockDuplicates();
    await renderPage();
    const region = await card();
    await expect.poll(() => region.getByRole("link").elements().length).toBeGreaterThan(0);
    await expect.element(region.getByRole("button", { name: /Retry all/ })).not.toBeInTheDocument();
    await expect.element(region.getByRole("button", { expanded: false })).not.toBeInTheDocument();
  });

  test("the duplicates skeleton opts out of motion under reduced motion", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    await renderPage();
    const skeletons = (await rendered(await card(false))).querySelectorAll<HTMLElement>(
      "[data-slot=skeleton]",
    );
    expect(skeletons).toHaveLength(3);
    for (const skeleton of skeletons)
      await expect.element(skeleton).toHaveClass("motion-reduce:animate-none");
  });
});

async function rendered(
  locator: Locator | HTMLElement | SVGElement,
): Promise<HTMLElement | SVGElement> {
  await expect.element(locator).toBeInTheDocument();
  return locator instanceof Element ? locator : locator.element();
}

async function renderedAll(locator: Locator): Promise<(HTMLElement | SVGElement)[]> {
  await expect.element(locator.first()).toBeInTheDocument();
  return locator.elements();
}
