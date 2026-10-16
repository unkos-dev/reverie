import { QueryClient, QueryClientProvider, focusManager } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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

import { DashboardPage } from "./DashboardPage";
import {
  GRACE_WINDOW_MS,
  POLL_INTERVAL_MS,
  STATUS_POLL_INTERVAL_MS,
} from "./ingestion/use-ingestion-monitor";

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

function renderPage(
  me: AuthMe = ADMIN_ME,
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
): QueryClient {
  client.setQueryData(queryKeys.auth.me(), me);
  const router = createMemoryRouter(
    [
      { path: "/admin/dashboard", element: <DashboardPage /> },
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
  render(<Wrapper />);
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
  await act(async () => {
    await vi.advanceTimersByTimeAsync(20);
  });
}

describe("scan control and notices", () => {
  test("starts idle with the Primary button, its hint and an empty status region", async () => {
    renderPage();
    expect(await screen.findByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
    expect(screen.getByText("Checks every file in the folder.")).toBeInTheDocument();
    const region = document.querySelector("[aria-live=polite]");
    expect(region).toBeEmptyDOMElement();
  });

  test("shows the requesting state while the scan is pending", async () => {
    vi.mocked(scanIngestion).mockReturnValue(new Promise(() => {}));
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    expect(screen.getByRole("button", { name: "Scanning" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(screen.getByText("Scanning the ingestion folder")).toBeInTheDocument();
    expect(
      screen.getByText("Looking at every file. This usually takes a few seconds."),
    ).toBeInTheDocument();
  });

  test("an accepted scan spells out the three counts and keeps zero figures", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 12,
      deferred: 0,
      suppressed: 5,
      monitor: "/m",
    });
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    expect(
      await screen.findByText("Scan started. 12 queued, 0 waiting, 5 unchanged."),
    ).toBeInTheDocument();
    expect(screen.getByText(/The scan queued new or changed files\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
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
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    expect(await screen.findByText(title)).toBeInTheDocument();
    expect(screen.getByText(detail)).toBeInTheDocument();
  });

  test("a failed request reads as a recoverable alert and offers Try again", async () => {
    vi.mocked(scanIngestion).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    vi.mocked(scanIngestion).mockResolvedValueOnce({
      queued: 0,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("The scan did not start")).toBeInTheDocument();
    await userEvent.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Nothing to import")).toBeInTheDocument();
    expect(screen.queryByText("The scan did not start")).not.toBeInTheDocument();
  });

  test("a network failure is treated like a failed request", async () => {
    vi.mocked(scanIngestion).mockRejectedValue(new TypeError("Failed to fetch"));
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    expect(await screen.findByText("The scan did not start")).toBeInTheDocument();
  });

  test("a 403 disables the button and says the account cannot scan", async () => {
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(403, null, "Forbidden", ""));
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    expect(await screen.findByText("This account cannot start a scan")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan ingestion folder" })).toBeDisabled();
  });

  test("a 401 says the session has ended", async () => {
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(401, null, "Unauthorized", ""));
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    expect(await screen.findByText("Your session has ended")).toBeInTheDocument();
    expect(screen.getByText("Sign in again to scan the ingestion folder.")).toBeInTheDocument();
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
    renderPage();
    await screen.findByText("7 files, 5 reasons");
    await userEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    expect(await screen.findByText("Files were sent for another try")).toBeInTheDocument();
    expect(screen.getByText(/Rejected files were left as they are/)).toBeInTheDocument();
    expect(await screen.findByText("2 files, 1 reason")).toBeInTheDocument();
    const card = screen.getByRole("region", { name: /Needs attention/ });
    expect(
      within(card).getByText(/Files marked Needs a change were sent for another try/),
    ).toBeInTheDocument();
  });

  test("does not claim a retry when the needs-change count did not fall", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 3,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    renderPage();
    await screen.findByText("7 files, 5 reasons");
    await userEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await screen.findByText(/Scan started/);
    expect(screen.queryByText("Files were sent for another try")).not.toBeInTheDocument();
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
      renderPage();
      await screen.findByText("7 files, 5 reasons");
      expect(screen.queryByRole("heading", { name: "Imports are paused" }) !== null).toBe(
        combination !== 0,
      );
      expect(screen.queryByText("New imports have been paused by an administrator.") !== null).toBe(
        Boolean(combination & 1),
      );
      expect(screen.queryByText("The library folder has no free space left.") !== null).toBe(
        Boolean(combination & 2),
      );
      expect(screen.queryByText("The ingestion worker is restarting.") !== null).toBe(
        Boolean(combination & 4),
      );
      expect(screen.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
      if (combination & 1)
        expect(screen.getByRole("link", { name: "Open Acquisition settings" })).toHaveAttribute(
          "href",
          "/admin/settings/acquisition",
        );
      if (combination & 2)
        expect(screen.getByText(/Dependency problem since/).querySelector("time")).toHaveAttribute(
          "datetime",
          "2026-10-16T03:00:00Z",
        );
      if (combination !== 0) {
        const notice = screen.getByRole("region", { name: "Imports are paused" });
        expect(notice.closest('[aria-live="polite"]')).not.toBeNull();
        expect(notice).toHaveTextContent(
          "Queued files are kept. Work already started can finish, and scans and recovery continue. New imports resume when Import files is on, dependencies are available and the worker is running.",
        );
        if (combination === 7) {
          const copy = notice.textContent;
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
      renderPage();
      expect(await screen.findByText(sentence)).toBeInTheDocument();
      expect(screen.getByText(check, { exact: false })).toBeInTheDocument();
    },
  );

  test("keeps scan feedback independent when a restart follows an accepted scan", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 2,
      deferred: 1,
      suppressed: 0,
      monitor: "/m",
    });
    const client = renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Scan ingestion folder" }));
    const title = "Scan started. 2 queued, 1 waiting, 0 unchanged.";
    await screen.findByText(title);
    act(() => {
      client.setQueryData(queryKeys.ingestion.status(), {
        blockers: [{ kind: "paused_by_admin" }, { kind: "restarting" }],
      });
    });
    expect(screen.getByText(title)).toBeInTheDocument();
    expect(await screen.findByText("The ingestion worker is restarting.")).toBeInTheDocument();
    expect(screen.getByText(/The scan queued new or changed files/)).toBeInTheDocument();
    expect(screen.queryByText(/being imported now/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
    await userEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await waitFor(() => {
      expect(scanIngestion).toHaveBeenCalledTimes(2);
    });
  });

  test("suppresses the running line during a pause and restores it after recovery", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [{ kind: "restarting" }] });
    const client = renderPage();
    await screen.findByText("The ingestion worker is restarting.");
    expect(screen.queryByText(/Latest activity\./)).not.toBeInTheDocument();
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [] });
    await act(async () => {
      await client.invalidateQueries({ queryKey: queryKeys.ingestion.status() });
    });
    expect(await screen.findByText(/Latest activity\./)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Imports are paused" })).not.toBeInTheDocument();
  });

  test("a failed refresh replaces stale blockers with the quiet failure and retry recovers", async () => {
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [{ kind: "paused_by_admin" }] });
    const client = renderPage();
    await screen.findByText("New imports have been paused by an administrator.");
    vi.mocked(getIngestionStatus).mockRejectedValue(new TypeError("network unavailable"));
    await act(async () => {
      await client.invalidateQueries({ queryKey: queryKeys.ingestion.status() });
    });
    expect(
      await screen.findByText("Could not check whether ingestion is running."),
    ).toBeInTheDocument();
    expect(screen.getByText("Files may still be importing.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Imports are paused" })).not.toBeInTheDocument();
    vi.mocked(getIngestionStatus).mockResolvedValue({ blockers: [] });
    await userEvent.click(
      within(screen.getByRole("status", { name: "Ingestion status" })).getByRole("button", {
        name: "Try again",
      }),
    );
    await waitFor(() => {
      expect(screen.queryByText("Files may still be importing.")).not.toBeInTheDocument();
    });
  });

  test.each([
    [401, "Your session has ended", "Sign in again to see ingestion status."],
    [403, "This account cannot see ingestion status", "Sign in as an administrator."],
  ])("status %i uses the permission treatment", async (code, title, detail) => {
    vi.mocked(getIngestionStatus).mockRejectedValue(
      new ApiError(code, null, "denied", "/private/path"),
    );
    renderPage();
    expect(await screen.findByRole("alert")).toHaveTextContent(title);
    expect(screen.getByText(detail)).toBeInTheDocument();
    expect(screen.queryByText("/private/path")).not.toBeInTheDocument();
  });

  test("shows no notice while the initial probe observation is loading", async () => {
    vi.mocked(getIngestionStatus).mockReturnValue(new Promise(() => {}));
    renderPage();
    await screen.findByText("7 files, 5 reasons");
    expect(screen.queryByRole("heading", { name: "Imports are paused" })).not.toBeInTheDocument();
    expect(screen.queryByText(/checking ingestion dependencies/i)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan ingestion folder" })).toBeEnabled();
  });
});

describe("Needs attention", () => {
  test("says nothing about files being sent for another try before any scan", async () => {
    renderPage();
    await screen.findByText("7 files, 5 reasons");
    expect(screen.queryByText(/were sent for another try/)).not.toBeInTheDocument();
  });

  test("headline total equals the sum of the group counts and excludes ignored files", async () => {
    renderPage();
    await screen.findByText("7 files, 5 reasons");
    const card = screen.getByRole("region", { name: /Needs attention/ });
    const shown = Array.from(card.querySelectorAll("button[aria-expanded]")).map((b) => {
      const label = /(\d+) files?$/.exec(b.textContent)?.[1];
      return { ignored: b.textContent.includes("Ignored"), count: Number(label) };
    });
    expect(shown).toHaveLength(6);
    expect(shown.find((g) => g.ignored)?.count).toBe(14);
    const attentionSum = shown.filter((g) => !g.ignored).reduce((sum, g) => sum + g.count, 0);
    expect(attentionSum).toBe(7);
  });

  test("orders groups by severity and gives each its action sentence once", async () => {
    renderPage();
    const card = await screen.findByRole("region", { name: /Needs attention/ });
    await within(card).findByText("The file is damaged");
    const titles = Array.from(card.querySelectorAll("button[aria-expanded] strong")).map(
      (el) => el.textContent,
    );
    expect(titles).toEqual([
      "The file is damaged",
      "Not a valid EPUB",
      "Reason not specified",
      "Needs a change",
      "Failed after several tries",
      "Ignored: format not accepted",
    ]);
    expect(screen.getAllByText("Download or export the book again, then scan.")).toHaveLength(1);
  });

  test("expands the first group on load and lists a two-reason file once with an Also line", async () => {
    renderPage();
    expect(await screen.findByText("Marlow - The Salt Archive.epub")).toBeInTheDocument();
    expect(screen.getByText("Also: Not a valid EPUB")).toBeInTheDocument();
    expect(screen.getAllByText("Marlow - The Salt Archive.epub")).toHaveLength(1);
    expect(screen.getByText("Showing 2 of 2")).toBeInTheDocument();
    expect(screen.getByText("incoming/older/")).toHaveClass("text-fg-muted");
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
    renderPage();
    expect(
      await screen.findByText("Also: Not a valid EPUB; Reason not specified"),
    ).toBeInTheDocument();
  });

  test("collapsed groups make no requests; expanding one requests its own reason", async () => {
    renderPage();
    await screen.findByText("Marlow - The Salt Archive.epub");
    expect(listInputs).toHaveBeenCalledTimes(1);
    await userEvent.click(screen.getByRole("button", { name: /Not a valid EPUB/ }));
    expect(await screen.findByText("Book 50.epub")).toBeInTheDocument();
    expect(vi.mocked(listInputs).mock.calls.map(([p]) => p.reason)).toEqual([
      "damaged",
      "invalid_structure",
    ]);
  });

  test("the group header is a button that toggles aria-expanded", async () => {
    renderPage();
    const button = await screen.findByRole("button", { name: /The file is damaged/ });
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(button).toHaveAttribute("aria-controls");
    await userEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("Marlow - The Salt Archive.epub")).not.toBeInTheDocument();
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
    renderPage();
    expect(await screen.findByText("Showing 2 of 3")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Show more" }));
    expect(await screen.findByText("Showing 3 of 3")).toBeInTheDocument();
    expect(screen.getAllByText("Book 2.epub")).toHaveLength(1);
    expect(screen.getByText("Book 3.epub")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
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
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Show more" }));
    const count = await screen.findByText("Showing 3 of 3");
    await waitFor(() => {
      expect(count).toHaveFocus();
    });
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
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Show more" }));
    await screen.findByText("Showing 4 of 5");
    expect(screen.getByRole("button", { name: "Show more" })).toHaveFocus();
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
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Show more" }));
    await screen.findByText("Could not load these files.");
    const card = screen.getByRole("region", { name: /Needs attention/ });
    const retry = within(card).getByRole("button", { name: "Try again" });
    await waitFor(() => {
      expect(retry).toHaveFocus();
    });
    vi.mocked(listInputs).mockResolvedValue({ items: [inputItem(3)], next_cursor: null });
    await userEvent.click(retry);
    const count = await screen.findByText("Showing 3 of 3");
    await waitFor(() => {
      expect(count).toHaveFocus();
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
    renderPage();
    const more = await screen.findByRole("button", { name: "Show more" });
    await userEvent.click(more);
    const loading = await screen.findByRole("button", { name: "Loading more" });
    expect(loading).toHaveAttribute("aria-disabled", "true");
    act(() => {
      resolveMore({ items: [inputItem(3)], next_cursor: null });
    });
    expect(await screen.findByText("Loaded 1 more file")).toBeInTheDocument();
  });

  test("an expanded group refetches its first page on window focus", async () => {
    renderPage();
    await screen.findByText("Marlow - The Salt Archive.epub");
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(77, { path: "incoming/Fresh Arrival.epub" })],
      next_cursor: null,
    });
    act(() => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    });
    expect(await screen.findByText("Fresh Arrival.epub")).toBeInTheDocument();
  });

  test("a second press on a pending scan sends no second request", async () => {
    vi.mocked(scanIngestion).mockReturnValue(new Promise(() => {}));
    renderPage();
    const button = await screen.findByRole("button", { name: "Scan ingestion folder" });
    await userEvent.click(button);
    const pending = await screen.findByRole("button", { name: "Scanning" });
    pending.focus();
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
    renderPage();
    await userEvent.click(await screen.findByRole("button", { name: "Show more" }));
    expect(await screen.findByText("Could not load these files.")).toBeInTheDocument();
    expect(screen.getByText("Book 1.epub")).toBeInTheDocument();
    vi.mocked(listInputs).mockResolvedValue({ items: [inputItem(3)], next_cursor: null });
    const card = screen.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(within(card).getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Book 3.epub")).toBeInTheDocument();
  });

  test("a failed first page shows the group error with a retry", async () => {
    vi.mocked(listInputs).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    renderPage();
    expect(await screen.findByText("Could not load these files.")).toBeInTheDocument();
    const card = screen.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(within(card).getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Marlow - The Salt Archive.epub")).toBeInTheDocument();
  });

  test("retrying a failed first page keeps keyboard focus on the group header", async () => {
    vi.mocked(listInputs).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    renderPage();
    await screen.findByText("Could not load these files.");
    const card = screen.getByRole("region", { name: /Needs attention/ });
    await userEvent.click(within(card).getByRole("button", { name: "Try again" }));
    await screen.findByText("Marlow - The Salt Archive.epub");
    expect(within(card).getByRole("button", { name: /The file is damaged/ })).toHaveFocus();
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
    renderPage();
    await screen.findByText("Showing 2 of 2");
    await userEvent.click(screen.getByRole("button", { name: /Ignored: format not accepted/ }));
    expect(await screen.findByText("cover.jpg")).toBeInTheDocument();
    expect(vi.mocked(listInputs).mock.calls.at(-1)?.[0]).toMatchObject({
      reason: "format_not_accepted",
    });
    expect(screen.getByText("14 files")).toBeInTheDocument();
  });

  test("a legacy row shows the masked class and never raw text", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "unspecified", count: 1 }], attention_total: 1 }),
    );
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(4, { primary_reason: "unspecified", reasons: ["unspecified"] })],
      next_cursor: null,
    });
    renderPage();
    expect(await screen.findByRole("button", { name: /Reason not specified/ })).toBeInTheDocument();
    expect(
      screen.getByText("Replace the file, or check the server log, then scan."),
    ).toBeInTheDocument();
  });

  test("a non-UTF-8 file name renders with its escapes and does not break the row", async () => {
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(5, { path: "incoming/Book \\xe9\\xff <b>.epub" })],
      next_cursor: null,
    });
    renderPage();
    expect(await screen.findByText("Book \\xe9\\xff <b>.epub")).toBeInTheDocument();
  });

  test("a file never tried shows no time", async () => {
    vi.mocked(listInputs).mockResolvedValue({
      items: [inputItem(6, { completed_at: null })],
      next_cursor: null,
    });
    renderPage();
    await screen.findByText("Book 6.epub");
    const card = screen.getByRole("region", { name: /Needs attention/ });
    expect(within(card).queryAllByText(/\d{1,2}:\d{2}/)).toHaveLength(0);
  });

  test("shows the empty message when no class needs attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue({ by_reason: [], attention_total: 0 });
    renderPage();
    expect(await screen.findByText("Nothing needs attention.")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Every file Reverie has seen was imported, is waiting its turn, or was ignored.",
      ),
    ).toBeInTheDocument();
    expect(listInputs).not.toHaveBeenCalled();
  });

  test("shows a busy skeleton with status text while counts load", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    renderPage();
    expect(await screen.findByText("Loading files that need attention")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: /Needs attention/ })).toHaveAttribute(
      "aria-busy",
      "true",
    );
  });

  test("the group chevron opts out of motion under reduced motion", async () => {
    renderPage();
    const chevron = (
      await screen.findByRole("button", { name: /The file is damaged/ })
    ).querySelector("svg");
    expect(chevron).toHaveClass("motion-reduce:transition-none");
  });

  test("the needs-attention skeletons opt out of motion under reduced motion", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    renderPage();
    const region = await screen.findByRole("region", { name: /Needs attention/ });
    const skeletons = region.querySelectorAll("[data-slot=skeleton]");
    expect(skeletons.length).toBeGreaterThan(0);
    for (const skeleton of skeletons) {
      expect(skeleton).toHaveClass("motion-reduce:animate-none");
    }
  });

  test("shows one error with a retry when the counts fail", async () => {
    vi.mocked(getInputCounts).mockRejectedValueOnce(new ApiError(500, null, "Internal", ""));
    renderPage();
    expect(await screen.findByText("Could not load this list.")).toBeInTheDocument();
    expect(screen.getByText("Your files are untouched. Try again.")).toBeInTheDocument();
    await userEvent.click(
      within(screen.getByRole("region", { name: /Needs attention/ })).getByRole("button", {
        name: "Try again",
      }),
    );
    expect(await screen.findByText("7 files, 5 reasons")).toBeInTheDocument();
  });

  test("keeps the existing metric cards", async () => {
    renderPage();
    expect(await screen.findByText("1,204")).toBeInTheDocument();
    expect(screen.getByText("Recent ingestion batches")).toBeInTheDocument();
  });

  test("a non-admin is redirected and nothing is requested", async () => {
    renderPage(ADULT_ME);
    expect(await screen.findByTestId("library-page")).toBeInTheDocument();
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
    renderPage();
    const card = await screen.findByRole("region", { name: /Enrichment problems/ });
    expect(await within(card).findByRole("heading")).toHaveTextContent("4 books, 3 reasons");
    const titles = Array.from(card.querySelectorAll("button[aria-expanded] strong")).map(
      (e) => e.textContent,
    );
    expect(titles).toEqual([
      "Hardcover: no record found",
      "Open Library: timed out",
      "Reason not specified",
    ]);
    expect(await within(card).findByText("Book title 1")).toBeInTheDocument();
    expect(within(card).getByText("Will retry, tried 3 times")).toBeInTheDocument();
    expect(within(card).getByText("Stopped retrying, tried 1 time")).toBeInTheDocument();
    expect(within(card).getByText("Also: Hardcover, no record found")).toBeInTheDocument();
    expect(vi.mocked(listEnrichmentFailures).mock.calls[0]?.[0]).toMatchObject({
      source: "hardcover",
      class: "not_found",
    });
  });

  test("never displays the attempt time, which can be a future retry anchor", async () => {
    renderPage();
    await screen.findByText("Book title 1");
    expect(screen.queryByText(/2099|Jan/)).not.toBeInTheDocument();
    const card = screen.getByRole("region", { name: /Enrichment problems/ });
    expect(within(card).queryAllByText(/\d{1,2}:\d{2}/)).toHaveLength(0);
  });

  test("a group with no source requests its class alone", async () => {
    renderPage();
    const card = await screen.findByRole("region", { name: /Enrichment problems/ });
    await userEvent.click(
      await within(card).findByRole("button", { name: /Reason not specified/ }),
    );
    await within(card).findAllByText("Book title 1");
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
    renderPage();
    const row = (await screen.findByText("Book title 1")).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(within(row).getByRole("button", { name: "Try again" }));
    expect(within(row).getByRole("button", { name: "Queuing" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    act(() => {
      finish();
    });
    expect(
      await within(row).findByText("Queued. It leaves this list once it runs."),
    ).toBeInTheDocument();
    expect(screen.getByText("Book title 1")).toBeInTheDocument();
    expect(triggerEnrichment).toHaveBeenCalledWith(failureItem(1).manifestation_id);
  });

  test("a retried book that fails again shows Try again once its row carries a newer attempt", async () => {
    vi.mocked(triggerEnrichment).mockResolvedValue();
    renderPage();
    const row = (await screen.findByText("Book title 1")).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(within(row).getByRole("button", { name: "Try again" }));
    await within(row).findByText("Queued. It leaves this list once it runs.");
    vi.mocked(listEnrichmentFailures).mockResolvedValue({
      items: [
        failureItem(1, { attempted_at: "2099-01-02T00:00:00Z", attempt_count: 1 }),
        failureItem(2),
      ],
      next_cursor: null,
    });
    act(() => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    });
    expect(await within(row).findByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(within(row).queryByText(/Queued\./)).not.toBeInTheDocument();
  });

  test("a refetch that returns the same attempt keeps the Queued confirmation", async () => {
    vi.mocked(triggerEnrichment).mockResolvedValue();
    renderPage();
    const row = (await screen.findByText("Book title 1")).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(within(row).getByRole("button", { name: "Try again" }));
    await within(row).findByText("Queued. It leaves this list once it runs.");
    const calls = vi.mocked(listEnrichmentFailures).mock.calls.length;
    act(() => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
    });
    await waitFor(() => {
      expect(vi.mocked(listEnrichmentFailures).mock.calls.length).toBeGreaterThan(calls);
    });
    expect(within(row).getByText("Queued. It leaves this list once it runs.")).toBeInTheDocument();
  });

  test("a successful retry moves focus to the Queued confirmation", async () => {
    vi.mocked(triggerEnrichment).mockResolvedValue();
    renderPage();
    const row = (await screen.findByText("Book title 1")).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(within(row).getByRole("button", { name: "Try again" }));
    const queued = await within(row).findByText("Queued. It leaves this list once it runs.");
    await waitFor(() => {
      expect(queued).toHaveFocus();
    });
  });

  test("a second press on a queuing retry sends no second request", async () => {
    vi.mocked(triggerEnrichment).mockReturnValue(new Promise(() => {}));
    renderPage();
    const row = (await screen.findByText("Book title 1")).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(within(row).getByRole("button", { name: "Try again" }));
    const queuing = within(row).getByRole("button", { name: "Queuing" });
    queuing.focus();
    await userEvent.keyboard("{Enter}");
    expect(triggerEnrichment).toHaveBeenCalledTimes(1);
  });

  test("a failed retry says so and offers Try again on the same row", async () => {
    vi.mocked(triggerEnrichment).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    renderPage();
    const row = (await screen.findByText("Book title 1")).closest("tr");
    if (row === null) throw new Error("row missing");
    await userEvent.click(within(row).getByRole("button", { name: "Try again" }));
    expect(await within(row).findByText("Could not queue it.")).toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Try again" })).toBeInTheDocument();
  });

  test("shows the empty, loading and error states", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockResolvedValueOnce({ by_failure: [], total: 0 });
    renderPage();
    expect(await screen.findByText("No enrichment problems.")).toBeInTheDocument();
    expect(
      screen.getByText("Every book was enriched, or is waiting its turn."),
    ).toBeInTheDocument();
  });

  test("shows a busy skeleton while loading", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockReturnValue(new Promise(() => {}));
    renderPage();
    expect(await screen.findByText("Loading books with enrichment problems")).toBeInTheDocument();
  });

  test("the enrichment skeletons opt out of motion under reduced motion", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockReturnValue(new Promise(() => {}));
    renderPage();
    const status = await screen.findByText("Loading books with enrichment problems");
    const skeletons = status.parentElement?.querySelectorAll("[data-slot=skeleton]") ?? [];
    expect(skeletons.length).toBeGreaterThan(0);
    for (const skeleton of skeletons) {
      expect(skeleton).toHaveClass("motion-reduce:animate-none");
    }
  });

  test("shows a retryable error when the counts fail", async () => {
    vi.mocked(getEnrichmentFailureCounts).mockRejectedValueOnce(
      new ApiError(500, null, "Internal", ""),
    );
    renderPage();
    const card = await screen.findByRole("region", { name: /Enrichment problems/ });
    expect(
      await within(card).findByText("Your books are untouched. Try again."),
    ).toBeInTheDocument();
    await userEvent.click(within(card).getByRole("button", { name: "Try again" }));
    expect(await within(card).findByRole("heading")).toHaveTextContent("4 books, 3 reasons");
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
    renderPage();
    await flush();
    expect(screen.queryByText(/Latest activity\./)).not.toBeInTheDocument();
    const before = {
      counts: vi.mocked(getInputCounts).mock.calls.length,
      activity: vi.mocked(getDashboardActivity).mock.calls.length,
      inputs: vi.mocked(listInputs).mock.calls.length,
    };
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    });
    expect(vi.mocked(getInputCounts).mock.calls.length - before.counts).toBe(2);
    expect(vi.mocked(getDashboardActivity).mock.calls.length - before.activity).toBe(2);
    expect(vi.mocked(listInputs).mock.calls.length - before.inputs).toBe(2);
  });

  test("status polls once every 30 seconds through pause and restart, suspends hidden reads and refetches on focus", async () => {
    vi.mocked(getIngestionStatus).mockResolvedValue({
      blockers: [{ kind: "paused_by_admin" }, { kind: "restarting" }],
    });
    renderPage();
    await flush();
    expect(getIngestionStatus).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(STATUS_POLL_INTERVAL_MS - 40);
    });
    expect(getIngestionStatus).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(40);
    });
    expect(getIngestionStatus).toHaveBeenCalledTimes(2);
    act(() => {
      focusManager.setFocused(false);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(STATUS_POLL_INTERVAL_MS * 2);
    });
    expect(getIngestionStatus).toHaveBeenCalledTimes(2);
    act(() => {
      focusManager.setFocused(true);
    });
    await flush();
    expect(getIngestionStatus).toHaveBeenCalledTimes(3);
  });

  test("status always refetches on mount even when a cached observation is fresh", async () => {
    const client = renderPage();
    await flush();
    cleanup();
    client.setQueryDefaults(queryKeys.ingestion.status(), { staleTime: Infinity });
    client.setQueryData(queryKeys.ingestion.status(), { blockers: [{ kind: "restarting" }] });
    renderPage(ADMIN_ME, client);
    await flush();
    expect(client.getQueryData(queryKeys.ingestion.status())).toEqual({ blockers: [] });
    expect(getIngestionStatus).toHaveBeenCalledTimes(2);
  });

  test("polls counts, activity and expanded groups every tick while a batch is running, one request each", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    renderPage();
    await flush();
    expect(screen.getByText(/Latest activity\./)).toBeInTheDocument();
    expect(screen.getByText(/Updating every few seconds/)).toBeInTheDocument();
    const before = {
      counts: vi.mocked(getInputCounts).mock.calls.length,
      activity: vi.mocked(getDashboardActivity).mock.calls.length,
      damaged: vi.mocked(listInputs).mock.calls.length,
    };
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    });
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
    renderPage();
    await flush();
    expect(screen.getByText(/2 skipped, 4 in progress/)).toBeInTheDocument();
  });

  test("does not poll when the latest batch has ended", async () => {
    renderPage();
    await flush();
    const counts = vi.mocked(getInputCounts).mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 10);
    });
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(counts);
  });

  test("announces that the latest activity finished once a running batch ends", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValueOnce(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    renderPage();
    await flush();
    expect(screen.queryByText("Latest activity has finished")).not.toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    });
    expect(screen.getByText("Latest activity has finished")).toBeInTheDocument();
    expect(
      screen.getByText(
        /12 files in the latest batch: 11 imported, 1 did not import\. Batches are not tied/,
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Updating every few seconds/)).not.toBeInTheDocument();
  });

  test("clears the waiting notice once a newer batch has ended, though no poll saw it running", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 8,
      suppressed: 0,
      monitor: "/m",
    });
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    expect(screen.getByText("Nothing is ready yet")).toBeInTheDocument();
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({
        batch_id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
        total: 1,
        completed: 1,
        failed: 0,
      }),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    });
    expect(screen.queryByText("Nothing is ready yet")).not.toBeInTheDocument();
    expect(screen.getByText("Latest activity has finished")).toBeInTheDocument();
  });

  test("keeps the waiting notice while the latest batch is the one seen before the scan", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 8,
      suppressed: 0,
      monitor: "/m",
    });
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 3);
    });
    expect(screen.getByText("Nothing is ready yet")).toBeInTheDocument();
    expect(screen.queryByText("Latest activity has finished")).not.toBeInTheDocument();
  });

  test("a 403 during a running batch outlives the batch and keeps the scan disabled", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValueOnce(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(403, null, "Forbidden", ""));
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    expect(screen.getByText("This account cannot start a scan")).toBeInTheDocument();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    });
    expect(screen.getByText("Latest activity has finished")).toBeInTheDocument();
    expect(screen.getByText("This account cannot start a scan")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Scan ingestion folder" })).toBeDisabled();
  });

  test("a failed scan alert outlives a batch that finishes", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValueOnce(
      batch({ ended_at: null, in_progress: 4, completed: 7 }),
    );
    vi.mocked(scanIngestion).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    });
    expect(screen.getByText("Latest activity has finished")).toBeInTheDocument();
    expect(screen.getByText("The scan did not start")).toBeInTheDocument();
  });

  test("polls for the two-minute window after a scan that queued files, then stops", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 2,
      deferred: 0,
      suppressed: 0,
      monitor: "/m",
    });
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    const afterScan = vi.mocked(getInputCounts).mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(GRACE_WINDOW_MS - POLL_INTERVAL_MS);
    });
    expect(vi.mocked(getInputCounts).mock.calls.length - afterScan).toBeGreaterThanOrEqual(30);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    });
    const stopped = vi.mocked(getInputCounts).mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 10);
    });
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(stopped);
  });

  test("refetches activity and counts on window focus once polling has stopped", async () => {
    renderPage();
    await flush();
    expect(screen.getByText("7 files, 5 reasons")).toBeInTheDocument();
    const counts = vi.mocked(getInputCounts).mock.calls.length;
    const activity = vi.mocked(getDashboardActivity).mock.calls.length;
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 9 }], attention_total: 9 }),
    );
    await act(async () => {
      focusManager.setFocused(false);
      focusManager.setFocused(true);
      await vi.advanceTimersByTimeAsync(20);
    });
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(counts + 1);
    expect(vi.mocked(getDashboardActivity).mock.calls).toHaveLength(activity + 1);
    expect(screen.getByText("9 files, 1 reason")).toBeInTheDocument();
  });

  test("a scan that queued and deferred nothing starts no poll", async () => {
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 0,
      suppressed: 9,
      monitor: "/m",
    });
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await flush();
    const afterScan = vi.mocked(getInputCounts).mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 10);
    });
    expect(vi.mocked(getInputCounts).mock.calls).toHaveLength(afterScan);
  });

  test("the enrichment lists are not polled", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    renderPage();
    await flush();
    const enrichment = vi.mocked(getEnrichmentFailureCounts).mock.calls.length;
    const lists = vi.mocked(listEnrichmentFailures).mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 5);
    });
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
    renderPage();
    await flush();
    fireEvent.click(screen.getByRole("button", { name: "Show more" }));
    await flush();
    const laterBefore = vi
      .mocked(listInputs)
      .mock.calls.filter(([p]) => p.cursor !== undefined).length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 4);
    });
    expect(vi.mocked(listInputs).mock.calls.filter(([p]) => p.cursor !== undefined)).toHaveLength(
      laterBefore,
    );
  });
});

describe("bulk retry group controls", () => {
  test("bulk retry only exposes eligible input actions as siblings and sends one pending command", async () => {
    vi.mocked(retryInputs).mockReturnValue(new Promise(() => {}));
    renderPage();
    const card = await screen.findByRole("region", { name: /Needs attention/ });
    const buttons = await within(card).findAllByRole("button", { name: /Retry/ });
    expect(buttons.map((button) => button.textContent)).toEqual(["Retry all 2", "Retry 1 file"]);
    const action = buttons[0];
    expect(action).toBeDefined();
    expect(action.parentElement?.querySelector("button[aria-expanded]")).toBeInTheDocument();
    action.focus();
    fireEvent.click(action);
    fireEvent.click(action);
    await waitFor(() => {
      expect(retryInputs).toHaveBeenCalledExactlyOnceWith("needs_change");
    });
    expect(await within(card).findByRole("button", { name: "Queuing 2" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(action).toHaveFocus();
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
    const client = renderPage();
    const action = await within(screen.getByRole("region", { name: /Needs attention/ })).findByRole(
      "button",
      { name: "Retry all 2" },
    );
    action.focus();
    fireEvent.click(action);
    await screen.findByText(/Queued 2\. Ingestion is paused/);
    fireEvent.click(screen.getByRole("button", { name: "Retry 1 file" }));
    await screen.findByText(/Queued 1\. Ingestion is paused/);
    act(() => {
      client.setQueryData(
        queryKeys.ingestion.counts(),
        countsFixture({ by_reason: [], attention_total: 0 }),
      );
    });
    expect(screen.getByText(/Queued 2\. Ingestion is paused/)).toBeInTheDocument();
    expect(screen.getByText(/Queued 1\. Ingestion is paused/)).toBeInTheDocument();
    expect(action).toHaveFocus();
    expect(action).toHaveAttribute("aria-disabled", "true");
    vi.mocked(scanIngestion).mockResolvedValue({
      queued: 0,
      deferred: 0,
      suppressed: 0,
      monitor: "/api/v1/dashboard/activity",
    });
    fireEvent.click(screen.getByRole("button", { name: "Scan ingestion folder" }));
    await waitFor(() => {
      expect(scanIngestion).toHaveBeenCalledOnce();
    });
    expect(screen.getByText(/Queued 2\. Ingestion is paused/)).toBeInTheDocument();
    act(() => {
      client.setQueryData(queryKeys.ingestion.counts(), countsFixture());
    });
    expect(
      await within(screen.getByRole("region", { name: /Needs attention/ })).findByRole("button", {
        name: "Retry all 2",
      }),
    ).toBeEnabled();
  });

  test("bulk retry retains canonical group order after refreshing to ignored files", async () => {
    vi.mocked(retryInputs).mockResolvedValue({ queued: 2, remaining: 0 });
    const client = renderPage();
    const card = screen.getByRole("region", { name: /Needs attention/ });
    const action = await within(card).findByRole("button", { name: "Retry all 2" });
    action.focus();
    fireEvent.click(action);
    await within(card).findByText("Queued 2");
    act(() => {
      client.setQueryData(
        queryKeys.ingestion.counts(),
        countsFixture({
          by_reason: [{ reason: "format_not_accepted", count: 1 }],
          attention_total: 0,
        }),
      );
    });
    await waitFor(() => {
      const toggles = within(card)
        .getAllByRole("button")
        .filter((button) => button.hasAttribute("aria-expanded"));
      expect(toggles).toHaveLength(2);
      expect(toggles[0]).toHaveTextContent("Needs a change");
      expect(toggles[1]).toHaveTextContent("Ignored");
    });
    expect(action).toHaveFocus();
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
      renderPage();
      const card = screen.getByRole("region", {
        name: noun === "files" ? /Needs attention/ : /Enrichment problems/,
      });
      fireEvent.click(await within(card).findByRole("button", { name: "Retry all 501" }));
      fireEvent.click(await within(card).findByRole("button", { name: "Retry the rest 1" }));
      expect(await within(card).findByRole("button", { name: "Queuing 1" })).toHaveAttribute(
        "aria-disabled",
        "true",
      );
    },
  );

  test("bulk retry partial batches use returned membership, then retain empty feedback", async () => {
    vi.mocked(retryInputs)
      .mockResolvedValueOnce({ queued: 500, remaining: 1 })
      .mockResolvedValueOnce({ queued: 1, remaining: 0 });
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "needs_change", count: 501 }], attention_total: 501 }),
    );
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Retry all 501" }));
    expect(await screen.findByText("Queued 500, 1 remaining")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry the rest 1" }));
    expect(await screen.findByText("Queued 1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Queued" })).toHaveAttribute("aria-disabled", "true");
    expect(retryInputs).toHaveBeenCalledTimes(2);
  });

  test.each([401, 403, 500])(
    "bulk retry reports %i truthfully without automatic replay",
    async (status) => {
      vi.mocked(retryInputs).mockRejectedValue(new ApiError(status, null, "failed", ""));
      renderPage();
      fireEvent.click(
        await within(screen.getByRole("region", { name: /Needs attention/ })).findByRole("button", {
          name: "Retry all 2",
        }),
      );
      const text =
        status === 401
          ? "Your session has ended. Sign in again to retry files."
          : status === 403
            ? "This account cannot retry files. Sign in as an administrator."
            : "Could not confirm whether these files were queued. Refresh the group before trying again.";
      expect(await screen.findByText(text)).toHaveAttribute("role", "alert");
      const retry = within(screen.getByRole("region", { name: /Needs attention/ })).getByRole(
        "button",
        { name: "Try again" },
      );
      if (status === 403) {
        fireEvent.click(retry);
        expect(retry).toHaveAttribute("aria-disabled", "true");
      }
      expect(retryInputs).toHaveBeenCalledOnce();
      expect(screen.queryByText(/Nothing was changed/)).not.toBeInTheDocument();
    },
  );

  test("bulk retry books keeps collapsed feedback through an empty refresh and rearms on fresh failure", async () => {
    vi.mocked(retryEnrichmentFailures).mockResolvedValue({ queued: 3, remaining: 0 });
    const client = renderPage();
    const card = await screen.findByRole("region", { name: /Enrichment problems/ });
    const [action] = await within(card).findAllByRole("button", { name: /Retry all/ });
    const count = Number(action.textContent.replace("Retry all ", ""));
    fireEvent.click(action);
    expect(await within(card).findByText("Queued 3")).toBeInTheDocument();
    act(() => {
      client.setQueryData(queryKeys.enrichmentFailures.counts(), { total: 0, by_failure: [] });
    });
    expect(within(card).getByText("Queued 3")).toBeInTheDocument();
    expect(action).toHaveAttribute("aria-disabled", "true");
    act(() => {
      client.setQueryData(queryKeys.enrichmentFailures.counts(), failureCountsFixture());
    });
    expect(
      await within(card).findByRole("button", { name: `Retry all ${String(count)}` }),
    ).toBeInTheDocument();
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
      const client = renderPage();
      const card = screen.getByRole("region", {
        name: noun === "files" ? /Needs attention/ : /Enrichment problems/,
      });
      fireEvent.click(await within(card).findByRole("button", { name: "Show more" }));
      await within(card).findByText(noun === "files" ? "continuation-2.epub" : "continuation-2");
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
      const action = within(card).getByRole("button", { name: "Retry all 5" });
      action.focus();
      fireEvent.click(action);
      await within(card).findByText(/Queued 2, 3 remaining/);
      await waitFor(() =>
        expect(
          within(card).queryByText(noun === "files" ? "continuation-2.epub" : "continuation-2"),
        ).not.toBeInTheDocument(),
      );
      expect(action).toHaveFocus();
      expect(client.getQueryData(untouchedKey)).toEqual(untouchedPage);
      expect(
        noun === "files"
          ? vi.mocked(listInputs).mock.calls.length
          : vi.mocked(listEnrichmentFailures).mock.calls.length,
      ).toBe(callsBefore);
      vi.mocked(listInputs).mockResolvedValue({ items: [file(4)], next_cursor: null });
      vi.mocked(listEnrichmentFailures).mockResolvedValue({ items: [book(4)], next_cursor: null });
      act(() => {
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
      });
      fireEvent.click(await within(card).findByRole("button", { name: "Show more" }));
      await within(card).findByText(noun === "files" ? "continuation-4.epub" : "continuation-4");
      expect(
        within(card).queryByText(noun === "files" ? "continuation-2.epub" : "continuation-2"),
      ).not.toBeInTheDocument();
      vi.mocked(retryInputs).mockResolvedValue({ queued: 3, remaining: 0 });
      vi.mocked(retryEnrichmentFailures).mockResolvedValue({ queued: 3, remaining: 0 });
      fireEvent.click(await within(card).findByRole("button", { name: "Retry all 3" }));
      await within(card).findByText(/Queued 3/);
      act(() => {
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
      });
      expect(await within(card).findByText("Showing 0 of 0")).toBeInTheDocument();
      expect(
        within(card).queryByText(noun === "files" ? "continuation-4.epub" : "continuation-4"),
      ).not.toBeInTheDocument();
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
      const client = renderPage();
      const card = screen.getByRole("region", {
        name: noun === "files" ? /Needs attention/ : /Enrichment problems/,
      });
      fireEvent.click(await within(card).findByRole("button", { name: "Show more" }));
      await within(card).findByText("Showing 2 of 3");
      fireEvent.click(within(card).getByRole("button", { name: "Retry all 3" }));
      await within(card).findByText(
        `Could not confirm whether these ${noun} were queued. Refresh the group before trying again.`,
      );
      expect(
        within(card).getByText(noun === "files" ? "retained-tail.epub" : "retained-tail"),
      ).toBeInTheDocument();
      expect(within(card).getByText("Showing 2 of 3")).toBeInTheDocument();
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
    renderPage();
    const card = screen.getByRole("region", { name: /Enrichment problems/ });
    const [action] = await within(card).findAllByRole("button", { name: "Retry 1 book" });
    fireEvent.click(action);
    const text =
      status === 401
        ? "Your session has ended. Sign in again to retry books."
        : status === 403
          ? "This account cannot retry books. Sign in as an administrator."
          : "Could not confirm whether these books were queued. Refresh the group before trying again.";
    expect(await within(card).findByText(text)).toHaveAttribute("role", "alert");
    expect(retryEnrichmentFailures).toHaveBeenCalledOnce();
    if (status === 403) {
      const button = within(card).getAllByRole("button", { name: "Try again" })[0];
      expect(button).toHaveAttribute("aria-disabled", "true");
      fireEvent.click(button);
      expect(retryEnrichmentFailures).toHaveBeenCalledOnce();
    }
  });

  test("bulk retry empty response stays visible while collapsed and does not fetch group rows", async () => {
    vi.mocked(retryInputs).mockResolvedValue({ queued: 0, remaining: 0 });
    renderPage();
    const card = screen.getByRole("region", { name: /Needs attention/ });
    fireEvent.click(await within(card).findByRole("button", { name: "Retry all 2" }));
    expect(
      await within(card).findByText("Nothing to retry. This group changed after the page loaded."),
    ).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "Queued" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
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
    renderPage();
    await flush();
    const card = screen.getByRole("region", { name: /Needs attention/ });
    const action = within(card).getByRole("button", { name: "Retry all 2" });
    action.focus();
    fireEvent.click(action);
    await flush();
    const before = vi.mocked(getInputCounts).mock.calls.length;
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [], attention_total: 0 }),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS + 20);
    });
    expect(getInputCounts).toHaveBeenCalledTimes(before + 1);
    expect(within(card).getByText("Queued 2")).toBeInTheDocument();
    expect(action).toHaveFocus();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(GRACE_WINDOW_MS + 20);
    });
    const stopped = vi.mocked(getInputCounts).mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(POLL_INTERVAL_MS * 2);
    });
    expect(getInputCounts).toHaveBeenCalledTimes(stopped);
    expect(within(card).getByText("Queued 2")).toBeInTheDocument();
  });
});
