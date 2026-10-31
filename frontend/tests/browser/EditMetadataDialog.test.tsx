import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { ViewTransition, type ReactElement } from "react";
import { beforeEach, expect, test, vi } from "vite-plus/test";

import { getBookMetadata, updateBookMetadata, type BookMetadata } from "@/api";

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

async function renderDialog(): Promise<{
  client: QueryClient;
  onOpenChange: ReturnType<typeof vi.fn>;
}> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
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
              fields={FIELDS}
            />
          </div>
        </ViewTransition>
      </QueryClientProvider>
    );
  }
  await render(<Wrapper />);
  return { client, onOpenChange };
}

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
