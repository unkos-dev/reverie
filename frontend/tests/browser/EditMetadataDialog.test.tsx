import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { ViewTransition, type ReactElement } from "react";
import { toast } from "sonner";
import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { ApiError, getBookMetadata, updateBookMetadata, type BookMetadata } from "@/api";
import { queryKeys } from "@/lib/query/keys";

import { EditMetadataDialog, type EditableField } from "@/pages/book/EditMetadataDialog";

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

async function renderDialog(
  fields: readonly EditableField[] = FIELDS,
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
): Promise<{
  client: QueryClient;
  onOpenChange: ReturnType<typeof vi.fn>;
  unmount: () => Promise<void>;
}> {
  const onOpenChange = vi.fn();
  function Wrapper(): ReactElement {
    return (
      <QueryClientProvider client={client}>
        <ViewTransition>
          <div>
            <EditMetadataDialog
              manifestationId="book-1"
              open
              onOpenChange={onOpenChange}
              fields={fields}
            />
          </div>
        </ViewTransition>
      </QueryClientProvider>
    );
  }
  const { unmount } = await render(<Wrapper />);
  return { client, onOpenChange, unmount };
}

async function submitEdit(): Promise<void> {
  const user = userEvent;
  const input = page.getByLabelText("Subtitle");
  await input.fill("");
  await user.type(input, "Updated Subtitle");
  await user.click(page.getByRole("button", { name: "Save" }));
}

describe("EditMetadataDialog", () => {
  test("seeds from metadata GET and reopens with saved values inside the cache freshness window", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
    });
    let finishRefresh: (value: BookMetadata) => void = () => {};
    const refreshedMetadata = new Promise<BookMetadata>((resolve) => {
      finishRefresh = resolve;
    });
    vi.mocked(getBookMetadata)
      .mockResolvedValueOnce(metadataFixture({ subtitle: "Fetched Subtitle" }))
      .mockReturnValueOnce(refreshedMetadata);
    vi.mocked(updateBookMetadata).mockResolvedValue({ fields: {} });
    const { onOpenChange, unmount } = await renderDialog(FIELDS, client);

    await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Fetched Subtitle");
    expect(getBookMetadata).toHaveBeenCalledWith("book-1", expect.anything());

    await submitEdit();
    await vi.waitFor(() => {
      expect(getBookMetadata).toHaveBeenCalledTimes(2);
    });
    expect(onOpenChange).not.toHaveBeenCalled();
    await expect.element(page.getByRole("button", { name: "Cancel" })).toBeDisabled();
    finishRefresh(metadataFixture({ subtitle: "Updated Subtitle" }));
    await vi.waitFor(() => {
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });
    await unmount();
    await renderDialog(FIELDS, client);

    await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
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
    const { client, onOpenChange } = await renderDialog();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");

    await submitEdit();

    await vi.waitFor(() => {
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
    const { onOpenChange } = await renderDialog();

    await submitEdit();
    await vi.waitFor(() => {
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

  test("a non-conflict failure shows the inline error and does not invalidate", async () => {
    vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
    vi.mocked(updateBookMetadata).mockRejectedValue(
      new ApiError(422, null, "Validation Error", "Subtitle is too long."),
    );
    const { client } = await renderDialog();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");

    await submitEdit();

    await expect
      .element(page.getByRole("alert"))
      .toHaveTextContent("Validation Error: Subtitle is too long.");
    expect(invalidateSpy).not.toHaveBeenCalled();
  });
});

test("awaits save, retains publication-date errors and other edits, and closes after correction", async () => {
  vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
  let reject: (error: Error) => void = () => {};
  vi.mocked(updateBookMetadata).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    }),
  );
  const { onOpenChange } = await renderDialog([
    ...FIELDS,
    { name: "pub_date", label: "Publication date" },
  ]);
  await page.getByLabelText("Publication date").fill("unknown");
  await submitEdit();
  await expect.element(page.getByRole("button", { name: "Save" })).toHaveAttribute("data-pending");
  await expect.element(page.getByRole("button", { name: "Cancel" })).toBeDisabled();
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  expect(onOpenChange).not.toHaveBeenCalled();
  reject(
    new ApiError(422, null, "Validation Error", "Invalid publication date.", {
      pub_date: "Use YYYY-MM-DD or YYYY. Leave empty if unknown.",
    }),
  );
  await expect
    .element(page.getByLabelText("Publication date"))
    .toHaveAttribute("aria-invalid", "true");
  await expect
    .element(page.getByLabelText("Publication date"))
    .toHaveAccessibleDescription(/Use YYYY-MM-DD or YYYY\. Leave empty if unknown\./);
  await expect.element(page.getByLabelText("Publication date")).toHaveValue("unknown");
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  await expect
    .element(page.getByRole("button", { name: "Save" }))
    .not.toHaveAttribute("data-pending");
  vi.mocked(updateBookMetadata).mockResolvedValueOnce({ fields: {} });
  await page.getByLabelText("Publication date").fill("2024-06");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await vi.waitFor(() => {
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
  expect(updateBookMetadata).toHaveBeenLastCalledWith("book-1", {
    subtitle: "Updated Subtitle",
    pub_date: "2024-06",
  });
});

test("cancelling a populated-field clear keeps the edit and sends nothing; confirmation saves null", async () => {
  vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
  vi.mocked(updateBookMetadata).mockResolvedValue({ fields: {} });
  await renderDialog();
  const input = page.getByLabelText("Subtitle");
  await expect.element(input).toHaveValue("Original Subtitle");
  await input.fill("");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  const confirmation = page.getByRole("alertdialog", { name: "Clear populated field?" });
  await expect.element(confirmation.getByRole("button", { name: "Cancel" })).toHaveFocus();
  await userEvent.click(confirmation.getByRole("button", { name: "Cancel" }));
  await expect.element(confirmation).not.toBeInTheDocument();
  await expect.element(input).toHaveValue("");
  expect(updateBookMetadata).not.toHaveBeenCalled();
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await userEvent.click(confirmation.getByRole("button", { name: "Clear and save" }));
  await vi.waitFor(() => {
    expect(updateBookMetadata).toHaveBeenCalledExactlyOnceWith("book-1", { subtitle: null });
  });
});

test("a pending clear suppresses duplicate activation and exposes a refusal with the edit retained", async () => {
  vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
  let reject: (error: Error) => void = () => {};
  vi.mocked(updateBookMetadata).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    }),
  );
  const { onOpenChange } = await renderDialog();
  const input = page.getByLabelText("Subtitle");
  await expect.element(input).toHaveValue("Original Subtitle");
  await input.fill("");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  const confirmation = page.getByRole("alertdialog", { name: "Clear populated field?" });
  const confirm = confirmation.getByRole("button", { name: "Clear and save" });
  await userEvent.click(confirm);
  await expect.element(confirm).toHaveAttribute("data-pending");
  await userEvent.click(confirm, { force: true });
  await userEvent.keyboard("{Enter}");
  expect(updateBookMetadata).toHaveBeenCalledExactlyOnceWith("book-1", { subtitle: null });
  expect(onOpenChange).not.toHaveBeenCalled();

  reject(new ApiError(422, null, "Validation Error", "Subtitle cannot be cleared."));
  await expect.element(confirmation).not.toBeInTheDocument();
  await expect.element(page.getByRole("alert")).toMatchTextContent("Subtitle cannot be cleared.");
  await expect.element(input).toHaveValue("");
  expect(updateBookMetadata).toHaveBeenCalledTimes(1);
  expect(onOpenChange).not.toHaveBeenCalled();

  vi.mocked(updateBookMetadata).mockResolvedValueOnce({ fields: {} });
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await userEvent.click(confirm);
  await vi.waitFor(() => {
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
  expect(updateBookMetadata).toHaveBeenCalledTimes(2);
  expect(updateBookMetadata).toHaveBeenLastCalledWith("book-1", { subtitle: null });
});
