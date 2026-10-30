import { render } from "vitest-browser-react";
import { page, userEvent } from "vite-plus/test/browser";
import { useState, type ReactElement } from "react";
import { expect, test } from "vite-plus/test";
import { Button } from "@/components/ui/button";

import {
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

function NestedDialog(): ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <Dialog isOpen={open} onOpenChange={setOpen}>
      <Button type="button">Open editor</Button>
      <DialogContent>
        <DialogTitle>Editor</DialogTitle>
        <Select aria-label="Status" defaultValue="new">
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem id="new">New</SelectItem>
            <SelectItem id="done">Done</SelectItem>
            <SelectItem id="disabled" isDisabled>
              Unavailable
            </SelectItem>
          </SelectContent>
        </Select>
        <Button slot="close">Finish</Button>
      </DialogContent>
    </Dialog>
  );
}

test("selects an enabled value by keyboard and closes through a native close action", async () => {
  const user = userEvent;
  await render(<NestedDialog />);
  await user.click(page.getByRole("button", { name: "Open editor" }));
  await user.click(page.getByRole("button", { name: /Status$/ }));
  await expect
    .element(page.getByRole("option", { name: "Unavailable" }))
    .toHaveAttribute("aria-disabled", "true");
  await user.keyboard("{ArrowDown}{Enter}");
  await expect.element(page.getByRole("button", { name: /Status$/ })).toHaveFocus();
  await expect.element(page.getByRole("button", { name: /Status$/ })).toHaveTextContent("Done");
  await user.click(page.getByRole("button", { name: "Finish" }));
  await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
});

test("dismisses an open dialog through its outside overlay", async () => {
  const user = userEvent;
  await render(<NestedDialog />);
  const opener = page.getByRole("button", { name: "Open editor" });
  await user.click(opener);
  await expect.element(page.getByRole("dialog", { name: "Editor" })).toBeInTheDocument();
  const overlay = document.querySelector(".fixed.inset-0");
  if (!(overlay instanceof HTMLElement)) throw new Error("overlay missing");
  await user.click(overlay, { position: { x: 8, y: 8 } });
  await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  await expect.element(opener).toHaveFocus();
});

function NestedConfirmation(): ReactElement {
  const [confirm, setConfirm] = useState(false);
  return (
    <Dialog>
      <Button type="button">Open parent</Button>
      <DialogContent>
        <DialogTitle>Parent</DialogTitle>
        <Button
          onPress={() => {
            setConfirm(true);
          }}
        >
          Delete item
        </Button>
        <AlertDialogContent isOpen={confirm} onOpenChange={setConfirm}>
          <AlertDialogTitle>Confirm deletion</AlertDialogTitle>
          <AlertDialogCancel>Cancel deletion</AlertDialogCancel>
        </AlertDialogContent>
      </DialogContent>
    </Dialog>
  );
}

test("a destructive confirmation blocks outside dismissal and cancellation preserves its parent", async () => {
  const user = userEvent;
  await render(<NestedConfirmation />);
  const opener = page.getByRole("button", { name: "Open parent" });
  await user.click(opener);
  const parent = page.getByRole("dialog", { name: "Parent" });
  const trigger = page.getByRole("button", { name: "Delete item" });
  await user.click(trigger);
  const confirmation = page.getByRole("alertdialog", { name: "Confirm deletion" });
  const cancel = page.getByRole("button", { name: "Cancel deletion" });
  await expect.element(cancel).toHaveFocus();
  const overlay = confirmation.element().closest(".fixed.inset-0");
  if (!(overlay instanceof HTMLElement)) throw new Error("overlay missing");
  await user.click(overlay, { position: { x: 8, y: 8 } });
  await expect.element(confirmation).toBeInTheDocument();
  await user.click(cancel);
  await expect.element(confirmation).not.toBeInTheDocument();
  await expect.element(parent).toBeInTheDocument();
  await expect.poll(() => parent.element().contains(document.activeElement)).toBe(true);
  await user.tab();
  await expect
    .poll(() =>
      parent
        .getByRole("button")
        .elements()
        .some((button) => button === document.activeElement),
    )
    .toBe(true);
  await user.keyboard("{Escape}");
  await expect.element(parent).not.toBeInTheDocument();
  await expect.element(opener).toHaveFocus();
});

function TemporaryTrigger(): ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <>
      {!open && (
        <button
          type="button"
          onClick={() => {
            setOpen(true);
          }}
        >
          Temporary opener
        </button>
      )}
      <button type="button">Remaining action</button>
      <DialogContent isOpen={open} onOpenChange={setOpen}>
        <DialogTitle>Temporary editor</DialogTitle>
      </DialogContent>
    </>
  );
}

test("a dialog remains dismissable when its opener unmounts", async () => {
  const user = userEvent;
  await render(<TemporaryTrigger />);
  const opener = page.getByRole("button", { name: "Temporary opener" });
  const openerElement = opener.element();
  await user.click(opener);
  await expect.element(page.getByRole("dialog", { name: "Temporary editor" })).toBeInTheDocument();
  await expect.element(opener).not.toBeInTheDocument();
  await user.keyboard("{Escape}");
  await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  expect(document.activeElement).not.toBe(openerElement);
  await user.click(page.getByRole("button", { name: "Remaining action" }));
  await expect.element(page.getByRole("button", { name: "Remaining action" })).toHaveFocus();
});
