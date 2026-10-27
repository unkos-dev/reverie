import { afterEach, expect, test } from "vite-plus/test";
import { page } from "vite-plus/test/browser";
import { render } from "vitest-browser-react";

import { FieldError } from "@/components/ui/field";
import { Table, TableHeader, TableHead, TableRow } from "@/components/ui/table";

const originalTheme = document.documentElement.dataset.theme;
afterEach(() => {
  if (originalTheme === undefined) delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = originalTheme;
});

test.each(["light", "dark"])("expected failures use neutral alert text in %s", async (theme) => {
  document.documentElement.dataset.theme = theme;
  await render(
    <section className="bg-canvas text-fg p-6">
      <p>Account details</p>
      <FieldError>Could not save. Try again.</FieldError>
    </section>,
  );
  const alert = page.getByRole("alert");
  await expect.element(alert).toHaveTextContent("Could not save. Try again.");
  expect(getComputedStyle(alert.element()).color).toBe(
    getComputedStyle(page.getByText("Account details").element()).color,
  );
  await page.screenshot({ path: `test-results/browser/field-feedback-${theme}.png` });
});

test("table heads expose column scope and allow an explicit row scope", async () => {
  await render(
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Title</TableHead>
          <TableHead scope="row">Book</TableHead>
        </TableRow>
      </TableHeader>
    </Table>,
  );
  await expect
    .element(page.getByRole("columnheader", { name: "Title" }))
    .toHaveAttribute("scope", "col");
  await expect
    .element(page.getByRole("rowheader", { name: "Book" }))
    .toHaveAttribute("scope", "row");
});
