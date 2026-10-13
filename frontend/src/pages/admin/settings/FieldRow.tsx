import { TriangleAlert } from "lucide-react";
import type { ReactElement, ReactNode } from "react";

import type { SettingsKey, SettingsSnapshot } from "@/api/settings";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

import { enabledFormatCount, shownBoolean, shownFormat, shownText } from "./editor";
import {
  deletionCopy,
  describeValue,
  formatBytes,
  parseMib,
  SUPPORTED_FORMATS,
  type Field,
  type IntField,
  type MibField,
  type SwitchField,
} from "./fields";
import { controlId, labelId } from "./ids";
import type { ConflictRow, SettingsEditor } from "./use-settings-editor";

const TAG =
  "ml-2 inline-block rounded-full border px-1.5 py-px align-middle font-mono text-[11px] uppercase leading-[1.4] tracking-[0.04em]";

function Tag({
  children,
  tone,
}: Readonly<{
  children: ReactNode;
  tone: "edited" | "plain" | "dashed";
}>): ReactElement {
  return (
    <span
      className={cn(
        TAG,
        tone === "edited" && "bg-accent-soft text-fg border-accent-text",
        tone === "plain" && "border-border-strong text-fg-muted",
        tone === "dashed" && "border-border-strong text-fg-muted border-dashed",
      )}
    >
      {children}
    </span>
  );
}

type Props = Readonly<{
  field: Field;
  base: SettingsSnapshot;
  editor: SettingsEditor;
}>;

function errorFor(editor: SettingsEditor, key: SettingsKey): string | null {
  const { notice } = editor;
  return notice.kind === "validation" ? (notice.errors[key] ?? null) : null;
}

function conflictRow(editor: SettingsEditor, key: SettingsKey): ConflictRow | null {
  const { notice } = editor;
  return notice.kind === "conflict" ? (notice.rows.find((row) => row.key === key) ?? null) : null;
}

function IntControl({ field, base, editor }: Readonly<Props & { field: IntField }>): ReactElement {
  const error = errorFor(editor, field.key);
  const busy = editor.phase !== "idle";
  const describedBy = [`help-${field.key}`, error === null ? null : `error-${field.key}`]
    .filter((id) => id !== null)
    .join(" ");
  return (
    <>
      <Input
        id={controlId(field.key)}
        inputMode="numeric"
        autoComplete="off"
        className="w-[7.5rem] tabular-nums"
        value={shownText(editor.draft, base.values, field.key)}
        aria-labelledby={labelId(field.key)}
        aria-describedby={describedBy}
        aria-invalid={error === null ? undefined : true}
        readOnly={busy}
        onChange={(event) => {
          editor.editText(field.key, event.target.value);
        }}
      />
      <span className="text-fg-muted ml-2 text-sm">{field.unit}</span>
    </>
  );
}

function MibControl({ field, base, editor }: Readonly<Props & { field: MibField }>): ReactElement {
  const error = errorFor(editor, field.key);
  const busy = editor.phase !== "idle";
  const text = shownText(editor.draft, base.values, field.key);
  const parsed = parseMib(text);
  const bytes = parsed.ok ? parsed.value : base.values.cover_max_bytes;
  const describedBy = [
    `help-${field.key}`,
    `bytes-${field.key}`,
    error === null ? null : `error-${field.key}`,
  ]
    .filter((id) => id !== null)
    .join(" ");
  return (
    <>
      <Input
        id={controlId(field.key)}
        inputMode="decimal"
        autoComplete="off"
        className="w-[7.5rem] tabular-nums"
        value={text}
        aria-labelledby={labelId(field.key)}
        aria-describedby={describedBy}
        aria-invalid={error === null ? undefined : true}
        readOnly={busy}
        onChange={(event) => {
          editor.editText(field.key, event.target.value);
        }}
      />
      <span className="text-fg-muted ml-2 text-sm">MiB</span>
      <div id={`bytes-${field.key}`} className="text-fg-muted mt-1.5 font-mono text-xs">
        {formatBytes(bytes)}
      </div>
    </>
  );
}

function SwitchControl({
  field,
  base,
  editor,
}: Readonly<Props & { field: SwitchField }>): ReactElement {
  const on = shownBoolean(editor.draft, base.values, field.key);
  return (
    <div className="inline-flex items-center gap-2.5">
      <Switch
        id={controlId(field.key)}
        className="motion-reduce:[&>[data-slot=switch-thumb]]:transition-none"
        checked={on}
        aria-labelledby={labelId(field.key)}
        aria-describedby={`help-${field.key}`}
        disabled={editor.phase !== "idle"}
        onCheckedChange={(value) => {
          editor.editBoolean(field.key, value);
        }}
      />
      <span
        aria-hidden="true"
        className="min-w-[2.2em] font-mono text-xs uppercase tracking-[0.04em]"
      >
        {on ? "On" : "Off"}
      </span>
    </div>
  );
}

function FormatsControl({ field, base, editor }: Props): ReactElement {
  if (SUPPORTED_FORMATS.length <= 1) {
    const only = SUPPORTED_FORMATS.map((format) => format.toUpperCase()).join(", ");
    return <p className="text-sm leading-normal">Reverie imports {only} files.</p>;
  }
  const lockId = `lock-${field.key}`;
  const lastEnabled = enabledFormatCount(editor.draft, base.values) === 1;
  return (
    <div role="group" aria-labelledby={labelId(field.key)}>
      {SUPPORTED_FORMATS.map((format, index) => {
        const checked = shownFormat(editor.draft, base.values, format);
        const locked = checked && lastEnabled;
        return (
          <Label
            key={format}
            className="flex w-fit cursor-pointer items-center gap-2 py-0.5 text-sm font-normal has-[[aria-disabled=true]]:cursor-not-allowed"
          >
            <Checkbox
              id={index === 0 ? controlId(field.key) : `${controlId(field.key)}-${format}`}
              checked={checked}
              disabled={editor.phase !== "idle"}
              aria-disabled={locked ? true : undefined}
              aria-describedby={locked ? lockId : undefined}
              onCheckedChange={(value) => {
                if (locked) return;
                editor.editFormat(format, value === true);
              }}
            />
            {format.toUpperCase()}
          </Label>
        );
      })}
      {lastEnabled ? (
        <p id={lockId} className="text-fg-muted mt-1.5 max-w-[52ch] text-[13px] leading-normal">
          This is the only format selected. At least one format must stay selected, or Reverie would
          have nothing to import.
        </p>
      ) : null}
    </div>
  );
}

function Control({ field, base, editor }: Props): ReactElement {
  switch (field.control) {
    case "switch":
      return <SwitchControl field={field} base={base} editor={editor} />;
    case "formats":
      return <FormatsControl field={field} base={base} editor={editor} />;
    case "mib":
      return <MibControl field={field} base={base} editor={editor} />;
    case "int":
      return <IntControl field={field} base={base} editor={editor} />;
  }
}

function ConflictChoices({
  row,
  editor,
}: Readonly<{
  row: ConflictRow;
  editor: SettingsEditor;
}>): ReactElement {
  const prefix = `choice-${row.key}`;
  return (
    <fieldset className="m-0 mt-2.5 border-0 p-0">
      <legend className="text-fg-muted p-0 text-[13px] leading-normal">
        Someone else saved {row.theirs} while you were editing. You set {row.mine}.
      </legend>
      <RadioGroup
        className="mt-1.5 flex w-auto gap-5"
        value={row.choice}
        onValueChange={(value) => {
          if (value === "mine" || value === "theirs") editor.choose(row.key, value);
        }}
      >
        <Label className="gap-2 text-sm font-normal">
          <RadioGroupItem id={`${prefix}-mine`} value="mine" />
          Keep mine ({row.mine})
        </Label>
        <Label className="gap-2 text-sm font-normal">
          <RadioGroupItem id={`${prefix}-theirs`} value="theirs" />
          Use theirs ({row.theirs})
        </Label>
      </RadioGroup>
    </fieldset>
  );
}

function Extras({ field, base, editor }: Props): ReactElement {
  const error = errorFor(editor, field.key);
  const row = conflictRow(editor, field.key);
  const dirty = editor.dirty.includes(field.key);
  const was = editor.elsewhere[field.key];
  const deletion = field.control === "switch" ? deletionCopy(field.key) : null;
  const deletionOn =
    field.control === "switch" &&
    deletion !== null &&
    dirty &&
    shownBoolean(editor.draft, base.values, field.key);
  const deletionNote = deletion?.note ?? "";
  const restart = base.restartRequiredFields.includes(field.key);
  return (
    <>
      {error === null ? null : (
        <div
          id={`error-${field.key}`}
          className="mt-1.5 flex items-baseline gap-1.5 text-[13px] font-medium leading-normal"
        >
          <TriangleAlert aria-hidden="true" className="size-3.5 shrink-0 translate-y-0.5" />
          {error}
        </div>
      )}
      {deletionOn ? (
        <div
          role="status"
          className="bg-accent-soft border-border-strong mt-2.5 rounded-lg border px-3 py-2.5"
        >
          <p className="text-fg text-sm leading-normal">
            When you save, Reverie starts deleting the original file from the ingestion folder{" "}
            {deletionNote}. This cannot be undone.
          </p>
        </div>
      ) : null}
      {restart ? (
        <div className="text-fg-muted mt-1.5 text-[13px] leading-normal">
          Takes effect after Reverie restarts.
        </div>
      ) : null}
      {row !== null ? <ConflictChoices row={row} editor={editor} /> : null}
      {row === null && was !== undefined ? (
        <div className="text-fg-muted mt-1.5 text-[13px] leading-normal">
          Changed by someone else while you were editing. It was {was}.
        </div>
      ) : null}
      {row === null && was === undefined && dirty ? (
        <div className="text-fg-muted mt-1.5 text-[13px] leading-normal">
          Saved value: {describeValue(field.key, base.values)}
        </div>
      ) : null}
    </>
  );
}

export function FieldRow({ field, base, editor }: Props): ReactElement {
  const dirty = editor.dirty.includes(field.key);
  const row = conflictRow(editor, field.key);
  const was = editor.elsewhere[field.key];
  const restart = base.restartRequiredFields.includes(field.key);
  return (
    <div
      data-field={field.key}
      className={cn(
        "-mx-3 grid grid-cols-[minmax(0,24rem)_minmax(0,1fr)] gap-8 rounded-lg px-3 py-4",
        dirty && row === null && "bg-accent-soft",
        row !== null && "bg-surface outline-border-strong outline outline-1",
      )}
    >
      <div>
        <div className="text-[15px] font-medium leading-normal">
          <span id={labelId(field.key)}>{field.label}</span>
          {dirty ? <Tag tone="edited">Edited</Tag> : null}
          {row !== null ? <Tag tone="edited">Conflict</Tag> : null}
          {was !== undefined ? <Tag tone="plain">Changed elsewhere</Tag> : null}
          {restart ? <Tag tone="dashed">Restart required</Tag> : null}
        </div>
        {field.help === undefined ? null : (
          <p
            id={`help-${field.key}`}
            className="text-fg-muted mt-0.5 max-w-[52ch] text-sm leading-normal"
          >
            {field.help}
          </p>
        )}
      </div>
      <div className="pt-0.5">
        <Control field={field} base={base} editor={editor} />
        <Extras field={field} base={base} editor={editor} />
      </div>
    </div>
  );
}
