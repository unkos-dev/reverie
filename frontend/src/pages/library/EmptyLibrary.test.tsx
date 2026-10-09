import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, test, vi } from "vite-plus/test";
import type { ReactElement } from "react";

import { ApiError } from "@/api";
import { getDashboardActivity } from "@/api/dashboard";
import { getInputCounts } from "@/api/ingestion";
import { ADMIN_ME, ADULT_ME, CHILD_ME, batch, countsFixture } from "@/__fixtures__/ingestion";
import type { AuthMe } from "@/hooks/useAuthMe";
import { queryKeys } from "@/lib/query/keys";

import { EmptyLibrary } from "./EmptyLibrary";

vi.mock("@/api/dashboard", () => ({ getDashboardActivity: vi.fn() }));
vi.mock("@/api/ingestion", () => ({ getInputCounts: vi.fn() }));

function renderEmpty(me: AuthMe | null): void {
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
  render(<Wrapper />);
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
    renderEmpty(ADULT_ME);
    expect(await screen.findByText("No books yet")).toBeInTheDocument();
    expect(
      screen.getByText(
        "There are no books here yet. Ask the person who manages this library to add some.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(getInputCounts).not.toHaveBeenCalled();
    expect(getDashboardActivity).not.toHaveBeenCalled();
  });

  test("a child account is treated as a reader", async () => {
    renderEmpty(CHILD_ME);
    expect(await screen.findByText(/Ask the person who manages this library/)).toBeInTheDocument();
    expect(getInputCounts).not.toHaveBeenCalled();
  });

  test("an admin with an empty folder gets the three steps and a link, without a folder path", async () => {
    renderEmpty(ADMIN_ME);
    expect(await screen.findByText("Where files go")).toBeInTheDocument();
    expect(screen.getByText("How it runs")).toBeInTheDocument();
    expect(screen.getByText("What to expect")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open Ingestion" })).toHaveAttribute(
      "href",
      "/admin/dashboard",
    );
    expect(document.body.textContent).not.toMatch(/\/(data|srv|mnt|var)\//);
    expect(document.querySelectorAll('[data-variant="default"]')).toHaveLength(0);
  });

  test("an admin sees a skeleton, not guidance, while the two reads are pending", async () => {
    vi.mocked(getInputCounts).mockReturnValue(new Promise(() => {}));
    renderEmpty(ADMIN_ME);
    expect(await screen.findByText("Checking the ingestion folder")).toBeInTheDocument();
    expect(screen.queryByText("Where files go")).not.toBeInTheDocument();
    expect(document.querySelector("[aria-busy=true]")).not.toBeNull();
  });

  test("a failed read falls back to the default guidance", async () => {
    vi.mocked(getInputCounts).mockRejectedValue(new ApiError(500, null, "Internal", ""));
    renderEmpty(ADMIN_ME);
    expect(await screen.findByText("Where files go")).toBeInTheDocument();
  });

  test("a running batch shows the under-way variant with a labelled progress bar", async () => {
    vi.mocked(getDashboardActivity).mockResolvedValue(
      batch({ ended_at: null, completed: 7, failed: 1, in_progress: 4 }),
    );
    renderEmpty(ADMIN_ME);
    expect(await screen.findByText("Ingestion is under way")).toBeInTheDocument();
    expect(
      screen.getByText("12 files are being checked. Books appear here as each one finishes."),
    ).toBeInTheDocument();
    expect(screen.getByText("7 imported, 1 did not import, 4 in progress")).toBeInTheDocument();
    const bar = screen.getByRole("progressbar", { name: "Files imported in the latest batch" });
    expect(bar).toHaveAttribute("aria-valuetext", "7 of 12 files imported");
    expect(screen.getByRole("link", { name: "Open Ingestion" })).toBeInTheDocument();
  });

  test("under way takes precedence over files needing attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(countsFixture());
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    renderEmpty(ADMIN_ME);
    expect(await screen.findByText("Ingestion is under way")).toBeInTheDocument();
    expect(screen.queryByText(/could not be imported/)).not.toBeInTheDocument();
  });

  test("an ended batch with files needing attention points at the one Primary action", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(countsFixture());
    vi.mocked(getDashboardActivity).mockResolvedValue(batch());
    renderEmpty(ADMIN_ME);
    expect(
      await screen.findByText(
        "7 files in the ingestion folder could not be imported. The Ingestion page shows why and what to do about each.",
      ),
    ).toBeInTheDocument();
    const link = screen.getByRole("link", { name: "Review these files" });
    expect(link).toHaveAttribute("href", "/admin/dashboard");
    expect(link).toHaveAttribute("data-variant", "default");
    expect(document.querySelectorAll('[data-variant="default"]')).toHaveLength(1);
  });

  test("one file needing attention reads in the singular", async () => {
    vi.mocked(getInputCounts).mockResolvedValue(
      countsFixture({ by_reason: [{ reason: "damaged", count: 1 }], attention_total: 1 }),
    );
    renderEmpty(ADMIN_ME);
    expect(
      await screen.findByText(/^1 file in the ingestion folder could not be imported\./),
    ).toBeInTheDocument();
  });

  test("ignored files alone do not count as needing attention", async () => {
    vi.mocked(getInputCounts).mockResolvedValue({
      by_reason: [{ reason: "format_not_accepted", count: 14 }],
      attention_total: 0,
    });
    renderEmpty(ADMIN_ME);
    expect(await screen.findByText("Where files go")).toBeInTheDocument();
  });

  test("never polls: each read happens once however long the page stays open", async () => {
    vi.useFakeTimers({
      toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"],
    });
    vi.mocked(getDashboardActivity).mockResolvedValue(batch({ ended_at: null, in_progress: 4 }));
    renderEmpty(ADMIN_ME);
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
