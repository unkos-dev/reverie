import type { ReactElement, ReactNode } from "react";
import { Link } from "react-router";

import { buttonVariants } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { restartLabels } from "./fields";
import type { Notice } from "./use-settings-editor";

type NoteProps = Readonly<{
  title: string;
  role: "status" | "alert";
  strong?: boolean;
  children: ReactNode;
  actions?: ReactNode;
}>;

export function Note({ title, role, strong = false, children, actions }: NoteProps): ReactElement {
  return (
    <div
      role={role}
      className={cn(
        "bg-surface mt-4 grid gap-1.5 rounded-lg border px-4 py-3.5",
        strong ? "border-fg-muted border-2" : "border-border-strong",
      )}
    >
      <h2 className="text-base font-semibold">{title}</h2>
      {children}
      {actions === undefined ? null : <div className="mt-2 flex flex-wrap gap-2">{actions}</div>}
    </div>
  );
}

export function NoteText({ children }: Readonly<{ children: ReactNode }>): ReactElement {
  return <p className="text-fg-muted max-w-[70ch] text-sm leading-normal">{children}</p>;
}

export function Notices({ notice }: Readonly<{ notice: Notice }>): ReactElement | null {
  switch (notice.kind) {
    case "none":
      return null;
    case "reapplying":
      return (
        <Note title="Settings changed while you were saving" role="status">
          <NoteText>
            Another admin saved settings first. Reverie is loading the latest settings and applying
            your changes to them. Nothing needs doing.
          </NoteText>
        </Note>
      );
    case "validation":
      return (
        <Note title="Settings were not saved" role="alert" strong>
          <NoteText>{notice.unmapped ?? "Review the field errors and try again."}</NoteText>
        </Note>
      );
    case "conflict":
      return (
        <Note title="Settings changed while you were editing" role="alert" strong>
          <NoteText>
            Another admin saved settings after you opened this page, so yours were not saved. Your
            edits are still here. Where you both changed the same setting, choose which value to
            keep. Settings only they changed now show their value.
          </NoteText>
        </Note>
      );
    case "forbidden":
      return (
        <Note
          title="Nothing was saved"
          role="alert"
          strong
          actions={
            <Link to="/library" className={buttonVariants({ variant: "outline", size: "sm" })}>
              Leave settings
            </Link>
          }
        >
          <NoteText>
            Your account no longer has administrator access. Your edits are still on this page. Ask
            an administrator to restore your access or to make the change for you.
          </NoteText>
        </Note>
      );
    case "failed":
      return (
        <Note title="Settings were not saved" role="alert" strong>
          <NoteText>
            {notice.reason === "network"
              ? "The server could not be reached. Your edits are still here."
              : "The server returned an error. Your edits are still here."}
          </NoteText>
        </Note>
      );
    case "saved":
      return (
        <Note title="Settings saved" role="status">
          <NoteText>
            {notice.reapplied
              ? "Another admin saved settings while you were editing. None of your changes overlapped with theirs, so Reverie reloaded the latest settings and saved your changes on top. "
              : ""}
            {notice.restartKeys.length > 0
              ? `Restart Reverie for ${restartLabels(notice.restartKeys)} to take effect. Everything else is in effect now.`
              : "Changes are in effect now. No restart is needed."}
          </NoteText>
          {notice.elsewhere === null ? null : <NoteText>{notice.elsewhere}</NoteText>}
        </Note>
      );
  }
}
