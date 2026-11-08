import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { ViewTransition, type ReactElement } from "react";
import { beforeEach, expect, test, vi } from "vite-plus/test";

import {
  ApiError,
  getBookMetadata,
  updateBookMetadata,
  type BookMetadata,
  type MetadataSnapshot,
  type OperationResponse,
  resolveMetadataOperation,
} from "@/api";

import { queryKeys } from "@/lib/query/keys";
import { EditMetadataDialog, type EditableField } from "@/pages/book/EditMetadataDialog";

vi.mock("@/api", async (importOriginal) => {
  const original = await importOriginal<Record<string, unknown>>();
  return {
    ...original,
    getBookMetadata: vi.fn(),
    updateBookMetadata: vi.fn(),
    resolveMetadataOperation: vi.fn(),
  };
});

beforeEach(() => {
  vi.resetAllMocks();
});

const FIELDS: readonly EditableField[] = [{ name: "subtitle", label: "Subtitle" }];

function metadataFixture(overrides: Partial<BookMetadata> = {}): MetadataSnapshot {
  return {
    etag: '"aaaaaaaaaaaaaaaaaaaaaa"',
    values: {
      title: "Original Title",
      subtitle: "Original Subtitle",
      description: null,
      language: null,
      publisher: null,
      pub_date: null,
      isbn_10: null,
      isbn_mismatch: false,
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
    },
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

test("seeds from metadata GET and reopens with saved values inside the cache freshness window", async () => {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: 30_000 } },
  });
  let finishRefresh: (value: MetadataSnapshot) => void = () => {};
  const refreshedMetadata = new Promise<MetadataSnapshot>((resolve) => {
    finishRefresh = resolve;
  });
  vi.mocked(getBookMetadata)
    .mockResolvedValueOnce(metadataFixture({ subtitle: "Fetched Subtitle" }))
    .mockReturnValueOnce(refreshedMetadata);
  vi.mocked(updateBookMetadata).mockResolvedValue({
    ...metadataFixture(),
    request_id: null,
    operations: [],
  });
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

test("awaits save, retains publication-date errors and other edits, and closes after correction", async () => {
  vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
  let reject: (error: Error) => void = () => {};
  vi.mocked(updateBookMetadata).mockReturnValueOnce(
    new Promise((_resolve, fail) => {
      reject = fail;
    }),
  );
  const { onOpenChange, client, unmount } = await renderDialog([
    ...FIELDS,
    { name: "pub_date", label: "Publication date" },
  ]);
  await page.getByLabelText("Publication date").fill("unknown");
  await submitEdit();
  await expect.element(page.getByRole("button", { name: "Save" })).toHaveAttribute("data-pending");
  await expect.element(page.getByRole("button", { name: "Cancel" })).toBeDisabled();
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  await expect.element(page.getByLabelText("Subtitle")).toHaveAttribute("readonly");
  await expect.element(page.getByLabelText("Publication date")).toHaveAttribute("readonly");
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
  await expect.element(page.getByLabelText("Publication date")).not.toHaveAttribute("readonly");
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  await expect
    .element(page.getByRole("button", { name: "Save" }))
    .not.toHaveAttribute("data-pending");
  const firstRequest = vi.mocked(updateBookMetadata).mock.calls[0];
  const newer = {
    ...metadataFixture({ subtitle: "Background" }),
    etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
  };
  client.setQueryData(queryKeys.books.metadata("book-1"), newer);
  vi.mocked(updateBookMetadata).mockRejectedValueOnce(new TypeError("Connection lost"));
  vi.mocked(resolveMetadataOperation).mockRejectedValueOnce(new TypeError("Offline"));
  await page.getByLabelText("Publication date").fill("2024-06");
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await vi.waitFor(() => {
    expect(resolveMetadataOperation).toHaveBeenCalled();
  });
  const retainedRequest = vi.mocked(updateBookMetadata).mock.calls[1];
  await expect.element(page.getByLabelText("Subtitle")).toHaveAttribute("readonly");
  await expect.element(page.getByRole("button", { name: "Save" })).toBeEnabled();
  expect(retainedRequest[2]).toBe(firstRequest[2]);
  expect(retainedRequest[3]).not.toBe(firstRequest[3]);
  vi.mocked(updateBookMetadata).mockRejectedValueOnce(
    new ApiError(
      412,
      "https://reverie.example/probs/if-match-mismatch",
      "Precondition failed",
      "Changed",
    ),
  );
  vi.mocked(resolveMetadataOperation).mockResolvedValueOnce({
    ...metadataFixture({ subtitle: "Updated Subtitle", pub_date: "2024-06" }),
    outcome: "saved",
    operations: [],
  });
  await userEvent.click(page.getByRole("button", { name: "Save" }));
  await vi.waitFor(() => {
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
  expect(vi.mocked(updateBookMetadata).mock.calls[2]?.[3]).toBe(retainedRequest[3]);
  expect(updateBookMetadata).toHaveBeenLastCalledWith(
    "book-1",
    {
      subtitle: "Updated Subtitle",
      pub_date: "2024-06",
    },
    '"aaaaaaaaaaaaaaaaaaaaaa"',
    expect.any(String),
  );
  await unmount();
  vi.mocked(updateBookMetadata).mockRejectedValueOnce(new TypeError("Lost acknowledgement"));
  vi.mocked(resolveMetadataOperation)
    .mockReset()
    .mockResolvedValueOnce({
      ...metadataFixture({ subtitle: "Updated Subtitle" }),
      outcome: "saved",
      operations: [],
    });
  const automatic = await renderDialog();
  await submitEdit();
  await vi.waitFor(() => {
    expect(automatic.onOpenChange).toHaveBeenCalledWith(false);
  });
  await automatic.unmount();
  let confirmEarlier: (value: OperationResponse) => void = () => {};
  const earlierReceipt = new Promise<OperationResponse>((resolve) => {
    confirmEarlier = resolve;
  });
  let rejectPending: (error: Error) => void = () => {};
  vi.mocked(updateBookMetadata).mockReturnValueOnce(
    new Promise((_resolve, reject) => {
      rejectPending = reject;
    }),
  );
  vi.mocked(resolveMetadataOperation).mockReset().mockReturnValueOnce(earlierReceipt);
  const retainedDraft = await renderDialog();
  await submitEdit();
  await expect.element(page.getByRole("button", { name: "Save" })).toHaveAttribute("data-pending");
  await expect.element(page.getByLabelText("Subtitle")).toHaveAttribute("readonly");
  await userEvent.type(page.getByLabelText("Subtitle"), "Newer unsaved input");
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  rejectPending(new TypeError("Lost acknowledgement"));
  await vi.waitFor(() => {
    expect(resolveMetadataOperation).toHaveBeenCalledTimes(1);
  });
  await expect
    .element(page.getByRole("button", { name: "Save" }))
    .not.toHaveAttribute("data-pending");
  await expect.element(page.getByLabelText("Subtitle")).toHaveAttribute("readonly");
  await expect.element(page.getByLabelText("Subtitle")).toHaveValue("Updated Subtitle");
  await expect.element(page.getByRole("button", { name: "Save" })).toBeEnabled();
  const confirmed = {
    ...metadataFixture({ subtitle: "Updated Subtitle" }),
    etag: '"bbbbbbbbbbbbbbbbbbbbbb"',
  };
  let finishConfirmation: (value: MetadataSnapshot) => void = () => {};
  vi.mocked(getBookMetadata).mockReturnValueOnce(
    new Promise((resolve) => {
      finishConfirmation = resolve;
    }),
  );
  const reads = vi.mocked(getBookMetadata).mock.calls.length;
  confirmEarlier({ ...confirmed, outcome: "saved", operations: [] });
  await vi.waitFor(() => {
    expect(getBookMetadata).toHaveBeenCalledTimes(reads + 1);
  });
  await expect.element(page.getByLabelText("Subtitle")).toHaveAttribute("readonly");
  expect(retainedDraft.onOpenChange).not.toHaveBeenCalled();
  finishConfirmation(confirmed);
  await vi.waitFor(() => {
    expect(retainedDraft.onOpenChange).toHaveBeenCalledWith(false);
  });
});

test("cancelling a populated-field clear keeps the edit and sends nothing; confirmation saves null", async () => {
  vi.mocked(getBookMetadata).mockResolvedValue(metadataFixture());
  vi.mocked(updateBookMetadata).mockResolvedValue({
    ...metadataFixture(),
    request_id: null,
    operations: [],
  });
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
    expect(updateBookMetadata).toHaveBeenCalledExactlyOnceWith(
      "book-1",
      { subtitle: null },
      '"aaaaaaaaaaaaaaaaaaaaaa"',
      expect.any(String),
    );
  });
});
