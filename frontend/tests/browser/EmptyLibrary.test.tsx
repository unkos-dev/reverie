import { act } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page } from "vite-plus/test/browser";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { getDashboardActivity } from "@/api/dashboard";
import { getInputCounts } from "@/api/ingestion";
import { ADMIN_ME, ADULT_ME, CHILD_ME, batch, countsFixture } from "@/__fixtures__/ingestion";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

import { EmptyLibrary } from "@/pages/library/EmptyLibrary";

vi.mock("@/api/dashboard", () => ({ getDashboardActivity: vi.fn() }));
vi.mock("@/api/ingestion", () => ({ getInputCounts: vi.fn() }));

async function renderEmpty(me: AuthMe | null): Promise<void> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (me !== null) client.setQueryData(queryKeys.auth.me(), me);
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <EmptyLibrary />
        </MemoryRouter>
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getInputCounts).mockResolvedValue({ by_reason: [], attention_total: 0 });
  vi.mocked(getDashboardActivity).mockResolvedValue({ batches: [] });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("EmptyLibrary", () => {
  test("a reader sees the holding copy, no action and no admin requests", async () => {
    await renderEmpty(ADULT_ME);
    await expect.element(page.getByText("No books yet")).toBeInTheDocument();
    await expect
      .element(
        page.getByText(
          "There are no books here yet. Ask the person who manages this library to add some.",
        ),
      )
      .toBeInTheDocument();
    await expect.element(page.getByRole("link")).not.toBeInTheDocument();
    await expect.element(page.getByRole("button")).not.toBeInTheDocument();
    expect(getInputCounts).not.toHaveBeenCalled();
    expect(getDashboardActivity).not.toHaveBeenCalled();
  });

  test("a child account is treated as a reader", async () => {
    await renderEmpty(CHILD_ME);
    await expect
      .element(page.getByText(/Ask the person who manages this library/))
      .toBeInTheDocument();
    expect(getInputCounts).not.toHaveBeenCalled();
  });

  test("an admin with an empty folder gets the three steps and a link, without a folder path", async () => {
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Where files go")).toBeInTheDocument();
    await expect.element(page.getByText("How it runs")).toBeInTheDocument();
    await expect.element(page.getByText("What to expect")).toBeInTheDocument();
    await expect
      .element(page.getByRole("link", { name: "Open Ingestion" }))
      .toHaveAttribute("href", "/admin/dashboard");
    expect(document.body.textContent).not.toMatch(/\/(data|srv|mnt|var)\//);
    expect(document.querySelectorAll<HTMLElement>('[data-variant="default"]')).toHaveLength(0);
  });

  test("an admin sees a skeleton, not guidance, while the two reads are pending", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Checking the ingestion folder")).toBeInTheDocument();
    await expect.element(page.getByText("Where files go")).not.toBeInTheDocument();
    expect(document.querySelector<HTMLElement>("[aria-busy=true]")).not.toBeNull();
  });

  test("the checking skeletons opt out of motion under reduced motion", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    await renderEmpty(ADMIN_ME);
    const status = page.getByText("Checking the ingestion folder");
    const skeletons =
      status.element().parentElement?.querySelectorAll<HTMLElement>("[data-slot=skeleton]") ?? [];
    expect(skeletons.length).toBeGreaterThan(0);
    for (const skeleton of skeletons) {
      await expect.element(skeleton).toHaveClass("motion-reduce:animate-none");
    }
  });

  test("a failed read falls back to the default guidance", async () => {
    vi.mocked(getInputCounts).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Where files go")).toBeInTheDocument();
  });

  test("failed activity with positive counts shows the default guidance, not attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(countsFixture());
    vi.mocked(getDashboardActivity).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Where files go")).toBeInTheDocument();
    await expect.element(page.getByText(/could not be imported/)).not.toBeInTheDocument();
  });

  test("failed counts with a running batch shows the default guidance, not progress", async () => {
    vi.mocked(getInputCounts).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Where files go")).toBeInTheDocument();
    await expect.element(page.getByText("Ingestion is under way")).not.toBeInTheDocument();
  });

  test("a running batch shows the under-way variant with a labelled progress bar", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({ ended_at: null, completed: 7, failed: 1, in_progress: 4 }),
    );
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Ingestion is under way")).toBeInTheDocument();
    await expect
      .element(
        page.getByText("12 files are being checked. Books appear here as each one finishes."),
      )
      .toBeInTheDocument();
    await expect
      .element(page.getByText("7 imported, 1 did not import, 4 in progress"))
      .toBeInTheDocument();
    const bar = page.getByRole("progressbar", { name: "Files imported in the latest batch" });
    await expect.element(bar).toHaveAttribute("aria-valuenow", "58");
    await expect.element(bar).toHaveAttribute("aria-valuemin", "0");
    await expect.element(bar).toHaveAttribute("aria-valuemax", "100");
    await expect.element(bar).toHaveAttribute("aria-valuetext", "7 of 12 files imported");
    await expect
      .element(bar)
      .toHaveClass("motion-reduce:[&>[data-slot=progress-indicator]]:transition-none");
    await expect.element(page.getByRole("link", { name: "Open Ingestion" })).toBeInTheDocument();
  });

  test("a batch with no completed files exposes zero progress", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({ ended_at: null, completed: 0, failed: 0, in_progress: 12 }),
    );
    await renderEmpty(ADMIN_ME);
    const bar = page.getByRole("progressbar", {
      name: "Files imported in the latest batch",
    });
    await expect.element(bar).toHaveAttribute("aria-valuenow", "0");
    await expect.element(bar).toHaveAttribute("aria-valuetext", "0 of 12 files imported");
    await expect
      .element(page.getByRole("link", { name: "Open Ingestion" }))
      .toHaveAttribute("href", "/admin/dashboard");
  });

  test("under way takes precedence over files needing attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(countsFixture());
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Ingestion is under way")).toBeInTheDocument();
    await expect.element(page.getByText(/could not be imported/)).not.toBeInTheDocument();
  });

  test("an ended batch with files needing attention points at the one Primary action", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(countsFixture());
    vi.mocked(getDashboardActivity).mockResolvedValue(batch());
    await renderEmpty(ADMIN_ME);
    await expect
      .element(
        page.getByText(
          "7 files in the ingestion folder could not be imported. The Ingestion page shows why and what to do about each.",
        ),
      )
      .toBeInTheDocument();
    const link = page.getByRole("link", { name: "Review these files" });
    await expect.element(link).toHaveAttribute("href", "/admin/dashboard");
    await expect.element(link).toHaveAttribute("data-variant", "default");
    expect(document.querySelectorAll<HTMLElement>('[data-variant="default"]')).toHaveLength(1);
  });

  test("one file needing attention reads in the singular", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 1 }], attention_total: 1 }),
    );
    await renderEmpty(ADMIN_ME);
    await expect
      .element(page.getByText(/^1 file in the ingestion folder could not be imported\./))
      .toBeInTheDocument();
  });

  test("ignored files alone do not count as needing attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue({
      by_reason: [{ reason: "format_not_accepted", count: 14 }],
      attention_total: 0,
    });
    await renderEmpty(ADMIN_ME);
    await expect.element(page.getByText("Where files go")).toBeInTheDocument();
  });

  test("never polls: each read happens once however long the page stays open", async () => {
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    });
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    await renderEmpty(ADMIN_ME);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(20);
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(5 * 60 * 1000);
    });
    expect(getInputCounts).toHaveBeenCalledTimes(1);
    expect(getDashboardActivity).toHaveBeenCalledTimes(1);
  });
});
