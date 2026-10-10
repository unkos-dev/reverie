import { QueryClient, QueryClientProvider, focusManager } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
} from "@/api/enrichment-failures";
import { getInputCounts, listInputs, scanIngestion } from "@/api/ingestion";
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
import { GRACE_WINDOW_MS, POLL_INTERVAL_MS } from "./ingestion/use-ingestion-monitor";

vi.mock("@/api/dashboard", () => ({
  getDashboardStats: vi.fn(),
  getDashboardActivity: vi.fn(),
}));
vi.mock("@/api/ingestion", async (importOriginal) => ({
  ...(await importOriginal<typeof IngestionApi>()),
  listInputs: vi.fn(),
  getInputCounts: vi.fn(),
  scanIngestion: vi.fn(),
}));
vi.mock("@/api/enrichment-failures", async (importOriginal) => ({
  ...(await importOriginal<typeof EnrichmentApi>()),
  listEnrichmentFailures: vi.fn(),
  getEnrichmentFailureCounts: vi.fn(),
  triggerEnrichment: vi.fn(),
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

function renderPage(me: AuthMe = ADMIN_ME): QueryClient {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
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
    expect(screen.getByText(/Queued files are being imported now\./)).toBeInTheDocument();
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
      "1 file is waiting and will be picked up automatically. You do not need to scan again.",
    ],
    [
      { queued: 0, deferred: 3, suppressed: 0 },
      "Nothing is ready yet",
      "3 files are waiting and will be picked up automatically. You do not need to scan again.",
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
    expect(within(card).queryByText(/\d:\d\d (am|pm)/)).not.toBeInTheDocument();
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
    expect(within(card).queryByText(/\d:\d\d (am|pm)/)).not.toBeInTheDocument();
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
    expect(vi.mocked(getInputCounts).mock.calls.length).toBe(counts);
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
    expect(vi.mocked(getInputCounts).mock.calls.length).toBe(stopped);
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
    expect(vi.mocked(getInputCounts).mock.calls.length).toBe(counts + 1);
    expect(vi.mocked(getDashboardActivity).mock.calls.length).toBe(activity + 1);
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
    expect(vi.mocked(getInputCounts).mock.calls.length).toBe(afterScan);
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
    expect(vi.mocked(getEnrichmentFailureCounts).mock.calls.length).toBe(enrichment);
    expect(vi.mocked(listEnrichmentFailures).mock.calls.length).toBe(lists);
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
    expect(vi.mocked(listInputs).mock.calls.filter(([p]) => p.cursor !== undefined).length).toBe(
      laterBefore,
    );
  });
});
