import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState, type ReactElement } from "react";
import { expect, test } from "vite-plus/test";
import { Button } from "./button";

import { AlertDialogCancel, AlertDialogContent, AlertDialogTitle } from "./alert-dialog";

import { Dialog, DialogContent, DialogTitle } from "./dialog";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "./select";

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

test("dismisses the nested popup before the dialog and restores trigger focus", async () => {
  const user = userEvent.setup();
  render(<NestedDialog />);
  const opener = screen.getByRole("button", { name: "Open editor" });
  await user.click(opener);
  expect(await screen.findByRole("dialog", { name: "Editor" })).toBeInTheDocument();
  const selectTrigger = screen.getByRole("button", { name: /Status$/ });
  await user.click(selectTrigger);
  expect(await screen.findByRole("listbox")).toBeInTheDocument();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("listbox")).not.toBeInTheDocument());
  expect(screen.getByRole("dialog", { name: "Editor" })).toBeInTheDocument();
  await waitFor(() => expect(selectTrigger).toHaveFocus());
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await waitFor(() => expect(opener).toHaveFocus());
});

test("selects an enabled value and closes through a native close action", async () => {
  const user = userEvent.setup();
  render(<NestedDialog />);
  await user.click(screen.getByRole("button", { name: "Open editor" }));
  await user.click(await screen.findByRole("button", { name: /Status$/ }));
  expect(await screen.findByRole("option", { name: "Unavailable" })).toHaveAttribute(
    "aria-disabled",
    "true",
  );
  await user.click(screen.getByRole("option", { name: "Done" }));
  expect(screen.getByRole("button", { name: /Status$/ })).toHaveTextContent("Done");
  await user.click(screen.getByRole("button", { name: "Finish" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
});

test("dismisses an open dialog through its outside overlay", async () => {
  const user = userEvent.setup();
  render(<NestedDialog />);
  const opener = screen.getByRole("button", { name: "Open editor" });
  await user.click(opener);
  await screen.findByRole("dialog", { name: "Editor" });
  await user.click(document.body);
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  await waitFor(() => expect(opener).toHaveFocus());
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
  const user = userEvent.setup();
  render(<NestedConfirmation />);
  await user.click(screen.getByRole("button", { name: "Open parent" }));
  const parent = await screen.findByRole("dialog", { name: "Parent" });
  const trigger = screen.getByRole("button", { name: "Delete item" });
  await user.click(trigger);
  const confirmation = await screen.findByRole("alertdialog", { name: "Confirm deletion" });
  const cancel = screen.getByRole("button", { name: "Cancel deletion" });
  await waitFor(() => expect(cancel).toHaveFocus());
  await user.click(document.body);
  expect(confirmation).toBeInTheDocument();
  await user.click(cancel);
  await waitFor(() => expect(confirmation).not.toBeInTheDocument());
  expect(parent).toBeInTheDocument();
  await waitFor(() => expect(trigger).toHaveFocus());
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
  const user = userEvent.setup();
  render(<TemporaryTrigger />);
  const opener = screen.getByRole("button", { name: "Temporary opener" });
  await user.click(opener);
  await screen.findByRole("dialog", { name: "Temporary editor" });
  expect(opener).not.toBeInTheDocument();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(document.activeElement).not.toBe(opener);
  await user.click(screen.getByRole("button", { name: "Remaining action" }));
  expect(screen.getByRole("button", { name: "Remaining action" })).toHaveFocus();
});
