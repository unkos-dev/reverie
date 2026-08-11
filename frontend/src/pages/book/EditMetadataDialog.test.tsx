import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { toast } from "sonner";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError, getBookMetadata, updateBookMetadata, type BookMetadata } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { EditMetadataDialog, type EditableField } from "./EditMetadataDialog";

vi.mock("@/api", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return { ...original, getBookMetadata: vi.fn(), updateBookMetadata: vi.fn() };
});
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

beforeEach(() => {
  vi.resetAllMocks();
});

const FIELDS: readonly EditableField[] = [{ name: "subtitle", label: "Subtitle" }];

function metadataFixture(overrides: Partial<BookMetadata> = {}): BookMetadata {
  return {
    title: "Original Title",
    subtitle: "Original Subtitle",
    description: null,
    language: null,
    publisher: null,
    pub_date: null,
    isbn_10: null,
    isbn_13: null,
    pages: null,
    content_rating: null,
    genres: [],
    moods: [],
    tags: [],
    contributors: { author: [], editor: [], translator: [] },
    work_identifiers: {},
    manifestation_identifiers: {},
    ...overrides,
  };
}

function renderDialog(): { client: QueryClient; onOpenChange: ReturnType<typeof vi.fn> } {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onOpenChange = vi.fn();
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <EditMetadataDialog
          manifestationId="book-1"
          open
          onOpenChange={onOpenChange}
          fields={FIELDS}
        />
      </QueryClientProvider>
    );
  }
  render(<Wrapper />);
  return { client, onOpenChange };
}

async function submitEdit(): Promise<void> {
  const user = userEvent.setup();
  const input = await screen.findByLabelText("Subtitle");
  await user.clear(input);
  await user.type(input, "Updated Subtitle");
  await user.click(screen.getByRole("button", { name: "Save" }));
}

describe("EditMetadataDialog", () => {
  test("fetches and seeds from a fresh GET rather than a caller-supplied canonical", async () => {
    vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture({ subtitle: "Fetched Subtitle" }));
    renderDialog();

    expect(await screen.findByLabelText("Subtitle")).toHaveValue("Fetched Subtitle");
    expect(getBookMetadata).toHaveBeenCalledWith("book-1", expect.anything());
  });

  test("a 412 conflict invalidates the book detail and metadata caches and offers a reload action instead of the generic error", async () => {
    vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
    const conflict = new ApiError(
      412,
      "https://reverie.example/probs/if-match-mismatch",
      "Precondition Failed",
      "",
    );
    vi.mocked(updateBookMetadata).mockRejectedValue(conflict);
    const { client, onOpenChange } = renderDialog();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");

    await submitEdit();

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith(
        "This book's metadata changed elsewhere.",
        expect.objectContaining({ action: expect.anything() as unknown }),
      );
    });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.detail("book-1") });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.books.metadata("book-1") });
    // Distinct from a normal save: the dialog does not close itself, only
    // the toast's own action does (exercised separately below).
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  test("the 412 toast's reload action closes the sheet", async () => {
    vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
    const conflict = new ApiError(
      412,
      "https://reverie.example/probs/if-match-mismatch",
      "Precondition Failed",
      "",
    );
    vi.mocked(updateBookMetadata).mockRejectedValue(conflict);
    const { onOpenChange } = renderDialog();

    await submitEdit();
    await waitFor(() => {
      expect(toast.error).toHaveBeenCalled();
    });

    const call = vi.mocked(toast.error).mock.calls[0];
    const action = call[1]?.action;
    if (action === null || typeof action !== "object" || !("onClick" in action)) {
      throw new Error("expected a reload action on the conflict toast");
    }
    action.onClick({} as never);

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  test("a non-conflict failure keeps the generic error toast and does not invalidate", async () => {
    vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
    vi.mocked(updateBookMetadata).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Subtitle is too long."),
    );
    const { client } = renderDialog();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");

    await submitEdit();

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Validation Error: Subtitle is too long.");
    });
    expect(invalidateSpy).not.toHaveBeenCalled();
  });
});
