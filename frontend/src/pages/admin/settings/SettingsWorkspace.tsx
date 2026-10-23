import { TriangleAlert } from "lucide-react";
import { useEffect, useMemo, useRef, type ReactElement, type SyntheticEvent } from "react";
import { Form } from "react-aria-components";
import { NavLink, useBlocker } from "react-router";

import type { SettingsKey, SettingsSnapshot } from "@/api/settings";
import {
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { formatTime } from "@/lib/format/datetime";
import { cn } from "@/lib/utils";

import { invalidTextKeys } from "./editor";
import { FieldRow } from "./FieldRow";
import { Notices } from "./Notices";
import {
  AREAS,
  areaById,
  areaSummary,
  deletionCopy,
  fieldsForArea,
  pluralise,
  type AreaId,
} from "./fields";
import { useSettingsEditor, type SettingsEditor } from "./use-settings-editor";

type Props = Readonly<{ area: AreaId; base: SettingsSnapshot }>;

const DIALOG_CONTENT = "gap-2 p-6 data-[size=default]:sm:max-w-[30rem]";
const DIALOG_FOOTER = "mx-0 mb-0 mt-4 border-t-0 bg-transparent p-0";

type ValidationErrors = Readonly<Record<string, string>>;

const NO_ERRORS: ValidationErrors = Object.freeze({});

function namedErrors(failed: Partial<Record<string, string>> | null): ValidationErrors {
  if (failed === null) return NO_ERRORS;
  const named: Record<string, string> = {};
  for (const [key, message] of Object.entries(failed)) {
    if (message !== undefined) named[key] = message;
  }
  return Object.keys(named).length === 0 ? NO_ERRORS : named;
}

function changeText(n: number): string {
  return `${String(n)} ${pluralise(n, "change", "changes")}`;
}

function SubNav({
  area,
  base,
  dirty,
}: Readonly<{
  area: AreaId;
  base: SettingsSnapshot;
  dirty: boolean;
}>): ReactElement {
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
    return <span role="status">Saved at {formatTime(notice.at)}.</span>;
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

function SaveBar({
  editor,
  resetForm,
}: Readonly<{ editor: SettingsEditor; resetForm: () => void }>): ReactElement {
  const conflict = editor.notice.kind === "conflict";
  const busy = editor.phase !== "idle";
  const pristine = !conflict && editor.dirty.length === 0;
  const saveLabel = conflict ? "Save my choices" : "Save changes";
  return (
    <div className="bg-surface border-border-strong sticky bottom-0 z-20 -mx-10 mt-auto flex items-center gap-3 border-t px-10 py-3">
      <div className="text-fg-muted mr-auto text-sm">{barSummary(editor)}</div>
      <Button
        type="button"
        variant="outline"
        isDisabled={pristine || busy}
        onPress={() => {
          resetForm();
          if (conflict) editor.discardAndReload();
          else editor.discard();
        }}
      >
        {conflict ? "Discard mine and reload" : "Discard changes"}
      </Button>
      <Button type="submit" isDisabled={pristine && !busy} isPending={busy}>
        {busy ? "Saving" : saveLabel}
      </Button>
    </div>
  );
}

function LeaveDialog({
  area,
  count,
  blocker,
}: Readonly<{
  area: AreaId;
  count: number;
  blocker: ReturnType<typeof useBlocker>;
}>): ReactElement {
  const title = areaById(area).title;
  return (
    <AlertDialogContent
      isOpen={blocker.state === "blocked"}
      onOpenChange={(open) => {
        if (!open && blocker.state === "blocked") blocker.reset();
      }}

      className={DIALOG_CONTENT}
    >
      <AlertDialogHeader>
        <AlertDialogTitle className="text-lg font-semibold">Leave without saving?</AlertDialogTitle>
        <AlertDialogDescription>
          You have {count} unsaved {pluralise(count, "change", "changes")} in {title}.{" "}
          {pluralise(count, "It", "They")} will be lost if you leave.
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter className={DIALOG_FOOTER}>
        <AlertDialogCancel>Stay on this page</AlertDialogCancel>
        <AlertDialogAction
          variant="outline"
          onPress={() => {
            if (blocker.state === "blocked") blocker.proceed();
          }}
        >
          Leave and discard changes
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
  );
}

function DeletionDialog({ editor }: Readonly<{ editor: SettingsEditor }>): ReactElement {
  const key = editor.pendingConfirm;
  const copy = key === null ? null : deletionCopy(key);
  return (
    <AlertDialogContent
      isOpen={key !== null}
      onOpenChange={(open) => {
        if (!open) editor.cancelDeletion();
      }}

      className={DIALOG_CONTENT}
    >
      <AlertDialogHeader>
        <AlertDialogTitle className="text-lg font-semibold">{copy?.title ?? ""}</AlertDialogTitle>
        <AlertDialogDescription>{copy?.body ?? ""}</AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter className={DIALOG_FOOTER}>
        <AlertDialogCancel>Keep source files</AlertDialogCancel>
        <AlertDialogAction
          className="bg-danger! text-fg-on-danger! border-danger! hover:bg-danger/90!"
          onPress={() => {
            editor.confirmDeletion();
          }}
        >
          <TriangleAlert aria-hidden="true" />
          Turn on removal
        </AlertDialogAction>
      </AlertDialogFooter>
    </AlertDialogContent>
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

  const formRef = useRef<HTMLFormElement>(null);
  const failed = editor.notice.kind === "validation" ? editor.notice.errors : null;
  const validationErrors = useMemo(() => namedErrors(failed), [failed]);

  function cancelValueReset(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
  }

  function resetForm(): void {
    formRef.current?.reset();
  }

  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    const { notice, draft } = editor;
    if (notice.kind !== "conflict") {
      editor.save();
      return;
    }
    const invalid: readonly SettingsKey[] = invalidTextKeys(draft);
    if (notice.rows.some((row) => row.choice === "theirs" && invalid.includes(row.key))) {
      resetForm();
    }
    editor.saveChoices();
  }

  const info = areaById(area);
  return (
    <>
      <Notices notice={editor.notice} />
      <div className="mt-4 grid flex-1 grid-cols-[15rem_minmax(0,1fr)] gap-12">
        <SubNav area={area} base={base} dirty={editor.dirty.length > 0} />
        <Form
          ref={formRef}
          className="flex flex-col"
          aria-label={`${info.title} settings`}
          validationErrors={validationErrors}
          onSubmit={submit}
          render={(props) => <form {...props} onResetCapture={cancelValueReset} />}
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
              <FieldRow key={field.key} field={field} base={editor.base} editor={editor} />
            ))}
          </section>
          <SaveBar editor={editor} resetForm={resetForm} />
        </Form>
      </div>
      <LeaveDialog area={area} count={editor.dirty.length} blocker={blocker} />
      <DeletionDialog editor={editor} />
    </>
  );
}
