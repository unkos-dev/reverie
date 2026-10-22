import type { ReactElement, ReactNode } from "react";
import {
  FieldError as AriaFieldError,
  Input as AriaInput,
  Label as AriaLabel,
  Text,
  TextField,
} from "react-aria-components";

import type { SettingsKey, SettingsSnapshot } from "@/api/settings";
import { Checkbox, CheckboxGroup } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { RadioGroup, Radio } from "@/components/ui/radio-group";
import { SwitchButton, SwitchField as AriaSwitchField } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

import { enabledFormatCount, shownBoolean, shownFormat, shownText } from "./editor";
import {
  deletionCopy,
  describeValue,
  formatBytes,
  parseMib,
  parseNumberField,
  SUPPORTED_FORMATS,
  type Field,
  type FormatsField,
  type IntField,
  type MibField,
  type SwitchField,
} from "./fields";
import { labelId } from "./ids";
import type { ConflictRow, SettingsEditor } from "./use-settings-editor";

const FIELD_ERROR = "text-destructive mt-1.5 text-sm font-normal";

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

function conflictRow(editor: SettingsEditor, key: SettingsKey): ConflictRow | null {
  const { notice } = editor;
  return notice.kind === "conflict" ? (notice.rows.find((row) => row.key === key) ?? null) : null;
}

function fieldErrorMessage(field: IntField | MibField, text: string): string | null {
  const parsed = parseNumberField(field, text);
  return parsed.ok ? null : parsed.message;
}

function NumberRow({
  field,
  base,
  editor,
}: Readonly<Props & { field: IntField | MibField }>): ReactElement {
  const text = shownText(editor.draft, base.values, field.key);
  const takesTheirs = conflictRow(editor, field.key)?.choice === "theirs";
  const isMib = field.control === "mib";
  const parsed = isMib ? parseMib(text) : null;
  const bytes = parsed?.ok === true ? parsed.value : base.values.cover_max_bytes;
  return (
    <TextField
      name={field.key}
      value={text}
      autoComplete="off"
      isReadOnly={editor.phase !== "idle"}
      aria-describedby={isMib ? `bytes-${field.key}` : undefined}
      validate={(value) => (takesTheirs ? null : fieldErrorMessage(field, value))}
      onChange={(next) => {
        editor.editText(field.key, next);
      }}
      className="contents"
    >
      <RowFrame
        field={field}
        base={base}
        editor={editor}
        label={<AriaLabel>{field.label}</AriaLabel>}
      >
        <AriaInput
          inputMode={isMib ? "decimal" : "numeric"}
          className="w-[7.5rem] tabular-nums"
          render={(props) => <Input {...props} />}
        />
        <span className="text-fg-muted ml-2 text-sm">{isMib ? "MiB" : field.unit}</span>
        {isMib ? (
          <div id={`bytes-${field.key}`} className="text-fg-muted mt-1.5 font-mono text-xs">
            {formatBytes(bytes)}
          </div>
        ) : null}
        <AriaFieldError className={FIELD_ERROR} />
      </RowFrame>
    </TextField>
  );
}

function SwitchRow({
  field,
  base,
  editor,
}: Readonly<Props & { field: SwitchField }>): ReactElement {
  const on = shownBoolean(editor.draft, base.values, field.key);
  return (
    <AriaSwitchField
      name={field.key}
      isSelected={on}
      aria-labelledby={labelId(field.key)}
      isDisabled={editor.phase !== "idle"}
      onChange={(value) => {
        editor.editBoolean(field.key, value);
      }}
    >
      <RowFrame
        field={field}
        base={base}
        editor={editor}
        label={<span id={labelId(field.key)}>{field.label}</span>}
      >
        <div className="inline-flex items-center gap-2.5">
          <SwitchButton className="motion-reduce:[&>[data-slot=switch-thumb]]:transition-none" />
          <span
            aria-hidden="true"
            className="min-w-[2.2em] font-mono text-xs uppercase tracking-[0.04em]"
          >
            {on ? "On" : "Off"}
          </span>
        </div>
        <AriaFieldError className={FIELD_ERROR} />
      </RowFrame>
    </AriaSwitchField>
  );
}

function FormatsRow({
  field,
  base,
  editor,
}: Readonly<Props & { field: FormatsField }>): ReactElement {
  const label = <span id={labelId(field.key)}>{field.label}</span>;
  if (SUPPORTED_FORMATS.length <= 1) {
    const only = SUPPORTED_FORMATS.map((format) => format.toUpperCase()).join(", ");
    return (
      <RowFrame field={field} base={base} editor={editor} label={label}>
        <p className="text-sm leading-normal">Reverie imports {only} files.</p>
      </RowFrame>
    );
  }
  const lockId = `lock-${field.key}`;
  const lastEnabled = enabledFormatCount(editor.draft, base.values) === 1;
  return (
    <CheckboxGroup
      name={field.key}
      aria-labelledby={labelId(field.key)}
      value={SUPPORTED_FORMATS.filter((format) => shownFormat(editor.draft, base.values, format))}
      isDisabled={editor.phase !== "idle"}
      onChange={(formats) => {
        for (const format of SUPPORTED_FORMATS) {
          const selected = formats.includes(format);
          if (selected !== shownFormat(editor.draft, base.values, format)) {
            editor.editFormat(format, selected);
          }
        }
      }}
      className="contents"
    >
      <RowFrame field={field} base={base} editor={editor} label={label}>
        {SUPPORTED_FORMATS.map((format) => (
          <Checkbox
            key={format}
            value={format}
            isReadOnly={shownFormat(editor.draft, base.values, format) && lastEnabled}
            aria-describedby={
              shownFormat(editor.draft, base.values, format) && lastEnabled ? lockId : undefined
            }
            className="cursor-pointer py-0.5 font-normal"
          >
            {format.toUpperCase()}
          </Checkbox>
        ))}
        {lastEnabled ? (
          <p id={lockId} className="text-fg-muted mt-1.5 max-w-[52ch] text-[13px] leading-normal">
            This is the only format selected. At least one format must stay selected, or Reverie
            would have nothing to import.
          </p>
        ) : null}
        <AriaFieldError className={FIELD_ERROR} />
      </RowFrame>
    </CheckboxGroup>
  );
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
      <legend id={`${prefix}-legend`} className="text-fg-muted p-0 text-[13px] leading-normal">
        Someone else saved {row.theirs} while you were editing. You set {row.mine}.
      </legend>
      <RadioGroup
        aria-labelledby={`${prefix}-legend`}
        orientation="horizontal"
        className="mt-1.5 flex w-auto gap-5"
        value={row.choice}
        onChange={(value) => {
          if (value === "mine" || value === "theirs") editor.choose(row.key, value);
        }}
      >
        <Radio id={`${prefix}-mine`} value="mine">
          Keep mine ({row.mine})
        </Radio>
        <Radio id={`${prefix}-theirs`} value="theirs">
          Use theirs ({row.theirs})
        </Radio>
      </RadioGroup>
    </fieldset>
  );
}

function Extras({ field, base, editor }: Props): ReactElement {
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

type FrameProps = Props & Readonly<{ label: ReactNode; children: ReactNode }>;

function RowFrame({ field, base, editor, label, children }: FrameProps): ReactElement {
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
          {label}
          {dirty ? <Tag tone="edited">Edited</Tag> : null}
          {row !== null ? <Tag tone="edited">Conflict</Tag> : null}
          {was !== undefined ? <Tag tone="plain">Changed elsewhere</Tag> : null}
          {restart ? <Tag tone="dashed">Restart required</Tag> : null}
        </div>
        {field.help === undefined ? null : (
          <Text
            id={`help-${field.key}`}
            slot="description"
            elementType="p"
            className="text-fg-muted mt-0.5 max-w-[52ch] text-sm leading-normal"
          >
            {field.help}
          </Text>
        )}
      </div>
      <div className="pt-0.5">
        {children}
        <Extras field={field} base={base} editor={editor} />
      </div>
    </div>
  );
}

export function FieldRow({ field, base, editor }: Props): ReactElement {
  switch (field.control) {
    case "switch":
      return <SwitchRow field={field} base={base} editor={editor} />;
    case "formats":
      return <FormatsRow field={field} base={base} editor={editor} />;
    case "mib":
    case "int":
      return <NumberRow field={field} base={base} editor={editor} />;
  }
}
