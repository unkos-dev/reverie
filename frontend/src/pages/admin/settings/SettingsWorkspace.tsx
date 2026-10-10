import { TriangleAlert } from "lucide-react";
import { useEffect, useRef, type ReactElement } from "react";
import { NavLink, useBlocker } from "react-router";

import type { SettingsSnapshot } from "@/api/settings";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { FieldRow } from "./FieldRow";
import { Notices } from "./Notices";
import {
  AREAS,
  areaById,
  areaSummary,
  deletionCopy,
  fieldCountText,
  fieldsForArea,
  pluralise,
  type AreaId,
  type BooleanKey,
} from "./fields";
import { controlId } from "./ids";
import { formatClock } from "./time";
import { useSettingsEditor, type SettingsEditor } from "./use-settings-editor";

type Props = { area: AreaId; base: SettingsSnapshot };

const DIALOG_CONTENT = "gap-2 p-6 data-[size=default]:sm:max-w-[30rem]";
const DIALOG_FOOTER = "mx-0 mb-0 mt-4 border-t-0 bg-transparent p-0";

function changeText(n: number): string {
  return `${String(n)} ${pluralise(n, "change", "changes")}`;
}

function SubNav({
  area,
  base,
  dirty,
}: {
  area: AreaId;
  base: SettingsSnapshot;
  dirty: boolean;
}): ReactElement {
  return (
    <nav aria-label="Settings areas" className="sticky top-16 self-start">
      {AREAS.map((item) => (
        <NavLink
          key={item.id}
          to={`/admin/settings/${item.id}`}
          className={({ isActive }) =>
            cn(
              "block rounded-md px-2.5 py-2 leading-normal",
              isActive ? "bg-surface text-fg" : "text-fg-muted hover:bg-surface hover:text-fg",
            )
          }
        >
          <span className="flex justify-between text-sm font-medium leading-normal">
            {item.title}
            {dirty && item.id === area ? (
              <span className="bg-accent-soft text-fg border-accent-text rounded-full border px-1.5 py-px font-mono text-[11px] uppercase leading-[1.4] tracking-[0.04em]">
                Edited
              </span>
            ) : null}
          </span>
          <span className="text-fg-muted block text-xs leading-normal">
            {areaSummary(item.id, base.values)}
          </span>
        </NavLink>
      ))}
    </nav>
  );
}

function barSummary(editor: SettingsEditor): ReactElement {
  const n = editor.dirty.length;
  const { notice, phase } = editor;
  if (phase !== "idle") return <span role="status">Saving {changeText(n)}</span>;
  if (notice.kind === "saved" && n === 0)
    return <span role="status">Saved at {formatClock(notice.at)}.</span>;
  if (notice.kind === "validation") {
    const count = Math.max(1, Object.keys(notice.errors).length);
    return (
      <span>
        <strong className="text-fg font-semibold">Not saved.</strong> {fieldCountText(count)}.
      </span>
    );
  }
  if (notice.kind === "conflict") {
    const c = notice.rows.length;
    return (
      <span>
        <strong className="text-fg font-semibold">Not saved.</strong>{" "}
        {c === 0
          ? "Settings were changed by someone else."
          : `${String(c)} ${pluralise(c, "setting was", "settings were")} changed by someone else.`}
      </span>
    );
  }
  if (n === 0) return <span>No unsaved changes.</span>;
  return (
    <span>
      <strong className="text-fg font-semibold">
        {n} unsaved {pluralise(n, "change", "changes")}
      </strong>
    </span>
  );
}

function SaveBar({ editor }: { editor: SettingsEditor }): ReactElement {
  const conflict = editor.notice.kind === "conflict";
  const busy = editor.phase !== "idle";
  const idle = editor.dirty.length === 0 || busy;
  return (
    <div className="bg-surface border-border-strong sticky bottom-0 z-20 -mx-10 mt-8 flex items-center gap-3 border-t px-10 py-3">
      <div className="text-fg-muted mr-auto text-sm">{barSummary(editor)}</div>
      {conflict ? (
        <Button type="button" variant="outline" disabled={busy} onClick={editor.discardAndReload}>
          Discard mine and reload
        </Button>
      ) : (
        <Button type="button" variant="outline" disabled={idle} onClick={editor.discard}>
          Discard changes
        </Button>
      )}
      <Button type="submit" disabled={conflict ? busy : idle} aria-busy={busy ? true : undefined}>
        {busy ? "Saving" : conflict ? "Save my choices" : "Save changes"}
      </Button>
    </div>
  );
}

function LeaveDialog({
  area,
  count,
  blocker,
}: {
  area: AreaId;
  count: number;
  blocker: ReturnType<typeof useBlocker>;
}): ReactElement {
  const title = areaById(area).title;
  const opener = useRef<HTMLElement | null>(null);
  return (
    <AlertDialog
      open={blocker.state === "blocked"}
      onOpenChange={(open) => {
        if (!open && blocker.state === "blocked") blocker.reset();
      }}
    >
      <AlertDialogContent
        className={DIALOG_CONTENT}
        onOpenAutoFocus={() => {
          opener.current =
            document.activeElement instanceof HTMLElement ? document.activeElement : null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          opener.current?.focus();
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="text-lg font-semibold">
            Leave without saving?
          </AlertDialogTitle>
          <AlertDialogDescription>
            You have {count} unsaved {pluralise(count, "change", "changes")} in {title}.{" "}
            {pluralise(count, "It", "They")} will be lost if you leave.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className={DIALOG_FOOTER}>
          <AlertDialogCancel>Stay on this page</AlertDialogCancel>
          <AlertDialogAction
            variant="outline"
            onClick={(event) => {
              event.preventDefault();
              if (blocker.state === "blocked") blocker.proceed();
            }}
          >
            Leave and discard changes
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

function DeletionDialog({ editor }: { editor: SettingsEditor }): ReactElement {
  const key = editor.pendingConfirm;
  const returnTo = useRef<BooleanKey | null>(null);
  useEffect(() => {
    if (key !== null) returnTo.current = key;
    return () => {};
  }, [key]);
  const copy = key === null ? null : deletionCopy(key);
  return (
    <AlertDialog
      open={key !== null}
      onOpenChange={(open) => {
        if (!open) editor.cancelDeletion();
      }}
    >
      <AlertDialogContent
        className={DIALOG_CONTENT}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          if (returnTo.current !== null)
            document.getElementById(controlId(returnTo.current))?.focus();
        }}
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="text-lg font-semibold">{copy?.title ?? ""}</AlertDialogTitle>
          <AlertDialogDescription>{copy?.body ?? ""}</AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter className={DIALOG_FOOTER}>
          <AlertDialogCancel>Keep source files</AlertDialogCancel>
          <AlertDialogAction
            className="bg-danger! text-fg-on-danger! border-danger! hover:bg-danger/90!"
            onClick={(event) => {
              event.preventDefault();
              editor.confirmDeletion();
            }}
          >
            <TriangleAlert aria-hidden="true" />
            Turn on removal
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

/**
 * One area's form, sub-navigation and dialogs. Keyed by area in the page, so
 * moving to another area (after the discard prompt) starts from a fresh draft.
 */
export function SettingsWorkspace({ area, base }: Props): ReactElement {
  const editor = useSettingsEditor(area, base);
  const guard = editor.dirty.length > 0 && editor.notice.kind !== "forbidden";
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      guard && currentLocation.pathname !== nextLocation.pathname,
  );

  useEffect(() => {
    if (!guard) return () => {};
    function warn(event: BeforeUnloadEvent): void {
      event.preventDefault();
    }
    window.addEventListener("beforeunload", warn);
    return () => {
      window.removeEventListener("beforeunload", warn);
    };
  }, [guard]);

  const info = areaById(area);
  return (
    <>
      <Notices notice={editor.notice} />
      <div className="mt-4 grid grid-cols-[15rem_minmax(0,1fr)] gap-12">
        <SubNav area={area} base={base} dirty={editor.dirty.length > 0} />
        <form
          aria-label={`${info.title} settings`}
          noValidate
          onSubmit={(event) => {
            event.preventDefault();
            if (editor.notice.kind === "conflict") editor.saveChoices();
            else editor.save();
          }}
        >
          <section aria-labelledby={`heading-${area}`} className="pb-2">
            <header>
              <h2
                id={`heading-${area}`}
                className="font-display text-[22px] font-medium tracking-[-0.01em]"
              >
                {info.title}
              </h2>
              <p className="text-fg-muted mt-0.5 max-w-[70ch] text-sm leading-normal">
                {info.description}
              </p>
            </header>
            {fieldsForArea(area).map((field) => (
              <FieldRow key={field.key} field={field} base={base} editor={editor} />
            ))}
          </section>
          <SaveBar editor={editor} />
        </form>
      </div>
      <LeaveDialog area={area} count={editor.dirty.length} blocker={blocker} />
      <DeletionDialog editor={editor} />
    </>
  );
}
