/**
 * Per-column filter editors for the library filter builder. Each is a
 * controlled presentational form: it reads its slice of the filter state and
 * reports edits through `onChange`. Numeric and date inputs keep only their
 * raw drafts locally so incomplete or crossing edits do not enter URL state.
 */
import { type ReactElement, useId, useState } from "react";

import { z } from "zod";

import type { SuggestKind } from "@/api";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  isTextOp,
  type FilterState,
  type RangeFilter,
  type SetFilter,
  type TextFilter,
  type TextOp,
} from "@/routes/library-params";

import { TypeaheadMultiSelect, type TypeaheadOption } from "./TypeaheadMultiSelect";

export type { TextOp };

const TEXT_OP_LABELS: Record<TextOp, string> = {
  contains: "contains",
  eq: "equals",
  ne: "does not equal",
  empty: "is empty",
};

function deriveTextOp(value: TextFilter, ops: readonly TextOp[]): TextOp {
  if (value.empty !== undefined && ops.includes("empty")) return "empty";
  if (value.contains !== undefined) return "contains";
  if (value.eq !== undefined) return "eq";
  if (value.ne !== undefined) return "ne";
  return ops[0] ?? "contains";
}

function textFilterFor(op: TextOp, text: string): TextFilter {
  if (op === "empty") return { empty: true };
  if (text === "") return {};
  if (op === "contains") return { contains: text };
  if (op === "eq") return { eq: text };
  return { ne: text };
}

type TextFilterEditorProps = {
  value: TextFilter;
  ops: readonly TextOp[];
  onChange: (next: TextFilter) => void;
};

export function TextFilterEditor({ value, ops, onChange }: TextFilterEditorProps): ReactElement {
  const id = useId();
  const [op, setOp] = useState<TextOp>(() => deriveTextOp(value, ops));
  const committed = value.contains ?? value.eq ?? value.ne ?? "";
  // The box holds raw input, not the committed value. Committed values are
  // trimmed, so rendering one straight back would delete the space between
  // two words the instant it is typed and put every multi-word filter out
  // of reach. A value that is normalised on its way back to the input
  // needs a local buffer to survive the round trip; trimming is that
  // normalisation here.
  const [text, setText] = useState(committed);
  const [syncedCommitted, setSyncedCommitted] = useState(committed);
  // Resync only when the committed value moved somewhere this box did not
  // put it: a clear affordance, or navigation. Comparing it against what
  // the current text commits to tells that apart from our own write
  // landing. Render-phase state adjustment, the compiler-accepted
  // alternative to a sync effect.
  if (committed !== syncedCommitted) {
    setSyncedCommitted(committed);
    if (committed !== text.trim()) setText(committed);
  }

  function changeOp(next: TextOp): void {
    setOp(next);
    onChange(textFilterFor(next, text));
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-1">
        <Label htmlFor={`${id}-op`}>Operator</Label>
        <Select
          value={op}
          onValueChange={(next) => {
            if (isTextOp(next)) changeOp(next);
          }}
        >
          <SelectTrigger id={`${id}-op`} aria-label="Operator">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {ops.map((candidate) => (
              <SelectItem key={candidate} value={candidate}>
                {TEXT_OP_LABELS[candidate]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {op === "empty" ? null : (
        <div className="flex flex-col gap-1">
          <Label htmlFor={`${id}-value`}>Value</Label>
          <Input
            id={`${id}-value`}
            aria-label="Filter value"
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              onChange(textFilterFor(op, event.target.value));
            }}
          />
        </div>
      )}
    </div>
  );
}

type BoundValue = number | string;
type BoundSide = "lower" | "upper";

const INTEGER_DRAFT = z
  .string()
  .regex(/^-?\d+$/)
  .transform(Number)
  .refine((value) => Number.isFinite(value) && Number.isInteger(value));

const CALENDAR_DATE_DRAFT = z
  .string()
  .regex(/^\d{4}-\d{2}-\d{2}$/)
  .refine((raw) => {
    const [year, month, day] = raw.split("-").map(Number);
    if (year < 1 || year > 9999) return false;
    const date = new Date(0);
    date.setUTCHours(0, 0, 0, 0);
    date.setUTCFullYear(year, month - 1, day);
    return (
      date.getUTCFullYear() === year &&
      date.getUTCMonth() === month - 1 &&
      date.getUTCDate() === day
    );
  });

function parseIntegerDraft(raw: string): number | undefined {
  const parsed = INTEGER_DRAFT.safeParse(raw);
  return parsed.success ? parsed.data : undefined;
}

function parseCalendarDateDraft(raw: string): string | undefined {
  const parsed = CALENDAR_DATE_DRAFT.safeParse(raw);
  return parsed.success ? parsed.data : undefined;
}

type DraftBoundInputProps<T extends BoundValue> = {
  id: string;
  label: string;
  type: "number" | "date";
  side: BoundSide;
  value?: T;
  peer?: T;
  intrinsicMin?: T;
  intrinsicMax?: T;
  disabled?: boolean;
  parse: (raw: string) => T | undefined;
  format: (value: T) => string;
  compare: (left: T, right: T) => number;
  onChange: (next: T | undefined) => void;
};

function DraftBoundInput<T extends BoundValue>({
  id,
  label,
  type,
  side,
  value,
  peer,
  intrinsicMin,
  intrinsicMax,
  disabled = false,
  parse,
  format,
  compare,
  onChange,
}: DraftBoundInputProps<T>): ReactElement {
  const canonical = value === undefined ? "" : format(value);
  const [draft, setDraft] = useState(canonical);
  const [syncedCanonical, setSyncedCanonical] = useState(canonical);
  const [syncedDisabled, setSyncedDisabled] = useState(disabled);
  const [lastNotifiedDraft, setLastNotifiedDraft] = useState(canonical);
  const [badInput, setBadInput] = useState(false);
  const [correction, setCorrection] = useState("");

  if (canonical !== syncedCanonical) {
    if (canonical !== lastNotifiedDraft) setCorrection("");
    setSyncedCanonical(canonical);
    setLastNotifiedDraft(canonical);
    setBadInput(false);
    setDraft(canonical);
  }
  if (disabled !== syncedDisabled) {
    setCorrection("");
    setSyncedDisabled(disabled);
    setLastNotifiedDraft(disabled ? "" : canonical);
    setBadInput(false);
    setDraft(disabled ? "" : canonical);
  } else if (disabled && draft !== "") {
    setBadInput(false);
    setDraft("");
  }

  let inputMin = intrinsicMin;
  let inputMax = intrinsicMax;
  if (peer !== undefined) {
    if (side === "lower") {
      if (inputMax === undefined || compare(peer, inputMax) < 0) inputMax = peer;
    } else if (inputMin === undefined || compare(peer, inputMin) > 0) {
      inputMin = peer;
    }
  }

  function clamp(valueToClamp: T): T {
    let result = valueToClamp;
    if (inputMin !== undefined && compare(result, inputMin) < 0) result = inputMin;
    if (inputMax !== undefined && compare(result, inputMax) > 0) result = inputMax;
    return result;
  }

  function notify(next: T | undefined, nextDraft: string): void {
    if (lastNotifiedDraft === nextDraft) return;
    setLastNotifiedDraft(nextDraft);
    onChange(next);
  }

  function commitDraft(): void {
    if (badInput) {
      setBadInput(false);
      setLastNotifiedDraft(canonical);
      setDraft(canonical);
      return;
    }
    if (draft === "") {
      notify(undefined, "");
      return;
    }
    const parsed = parse(draft);
    if (parsed === undefined) {
      setLastNotifiedDraft(canonical);
      setDraft(canonical);
      return;
    }
    const legal = clamp(parsed);
    const legalDraft = format(legal);
    if (compare(parsed, legal) !== 0) {
      const limit = compare(parsed, legal) < 0 ? "minimum" : "maximum";
      setCorrection(`${label} changed to ${legalDraft}, the ${limit} allowed.`);
      setLastNotifiedDraft(legalDraft);
      setDraft(legalDraft);
      onChange(legal);
      return;
    }
    notify(parsed, draft);
  }

  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        type={type}
        className={type === "date" ? "px-2 text-sm" : undefined}
        min={inputMin}
        max={inputMax}
        disabled={disabled}
        value={draft}
        aria-describedby={correction ? `${id}-correction` : undefined}
        onChange={(event) => {
          setCorrection("");
          const raw = event.currentTarget.value;
          const isBadInput = event.currentTarget.validity.badInput;
          setDraft(raw);
          setBadInput(isBadInput);
          if (isBadInput) return;
          if (raw === "") {
            notify(undefined, "");
            return;
          }
          const parsed = parse(raw);
          if (parsed === undefined) return;
          const legal = clamp(parsed);
          if (compare(parsed, legal) === 0) notify(parsed, raw);
        }}
        onBlur={commitDraft}
        onKeyDown={(event) => {
          if (event.key === "Enter") commitDraft();
          if (event.key === "Escape") {
            setCorrection("");
            setBadInput(false);
            setLastNotifiedDraft(canonical);
            setDraft(canonical);
          }
        }}
      />
      <p id={`${id}-correction`} role="status" className="text-fg-muted text-sm empty:sr-only">
        {correction}
      </p>
    </div>
  );
}

type RangeFilterEditorProps = {
  value: RangeFilter;
  min?: number;
  max?: number;
  allowEmpty?: boolean;
  onChange: (next: RangeFilter) => void;
};

export function RangeFilterEditor({
  value,
  min,
  max,
  allowEmpty = false,
  onChange,
}: RangeFilterEditorProps): ReactElement {
  const id = useId();
  const isEmpty = value.empty === true;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-start gap-2">
        <DraftBoundInput
          id={`${id}-min`}
          label="Min"
          type="number"
          side="lower"
          value={value.gte}
          peer={value.lte}
          intrinsicMin={min}
          intrinsicMax={max}
          disabled={isEmpty}
          parse={parseIntegerDraft}
          format={String}
          compare={(left, right) => left - right}
          onChange={(gte) => {
            onChange({ ...value, gte });
          }}
        />
        <DraftBoundInput
          id={`${id}-max`}
          label="Max"
          type="number"
          side="upper"
          value={value.lte}
          peer={value.gte}
          intrinsicMin={min}
          intrinsicMax={max}
          disabled={isEmpty}
          parse={parseIntegerDraft}
          format={String}
          compare={(left, right) => left - right}
          onChange={(lte) => {
            onChange({ ...value, lte });
          }}
        />
      </div>
      {allowEmpty ? (
        <div className="flex items-center gap-2">
          <Checkbox
            id={`${id}-empty`}
            checked={isEmpty}
            onCheckedChange={(checked) => {
              onChange(checked === true ? { empty: true } : {});
            }}
          />
          <Label htmlFor={`${id}-empty`}>Has no value</Label>
        </div>
      ) : null}
    </div>
  );
}

type DateRangeEditorProps = {
  after?: string;
  before?: string;
  onChange: (next: { after?: string; before?: string }) => void;
};

export function DateRangeEditor({ after, before, onChange }: DateRangeEditorProps): ReactElement {
  const id = useId();
  return (
    <div className="flex items-start gap-2">
      <DraftBoundInput
        id={`${id}-after`}
        label="After"
        type="date"
        side="lower"
        value={after}
        peer={before}
        parse={parseCalendarDateDraft}
        format={(value) => value}
        compare={(left, right) => left.localeCompare(right)}
        onChange={(nextAfter) => {
          onChange({ after: nextAfter, before });
        }}
      />
      <DraftBoundInput
        id={`${id}-before`}
        label="Before"
        type="date"
        side="upper"
        value={before}
        peer={after}
        parse={parseCalendarDateDraft}
        format={(value) => value}
        compare={(left, right) => left.localeCompare(right)}
        onChange={(nextBefore) => {
          onChange({ after, before: nextBefore });
        }}
      />
    </div>
  );
}

const STATUS_OPTIONS = [
  { token: "unread", label: "Unread" },
  { token: "want_to_read", label: "Want to read" },
  { token: "reading", label: "Reading" },
  { token: "on_hold", label: "On hold" },
  { token: "finished", label: "Finished" },
  { token: "abandoned", label: "Abandoned" },
] as const;

type StatusEditorProps = {
  value: readonly string[];
  onChange: (next: string[]) => void;
};

export function StatusEditor({ value, onChange }: StatusEditorProps): ReactElement {
  const id = useId();
  function toggle(token: string, checked: boolean): void {
    const without = value.filter((current) => current !== token);
    onChange(checked ? [...without, token] : without);
  }

  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="sr-only">Reading status</legend>
      {STATUS_OPTIONS.map((option) => (
        <div key={option.token} className="flex items-center gap-2">
          <Checkbox
            id={`${id}-${option.token}`}
            checked={value.includes(option.token)}
            onCheckedChange={(checked) => {
              toggle(option.token, checked === true);
            }}
          />
          <Label htmlFor={`${id}-${option.token}`}>{option.label}</Label>
        </div>
      ))}
    </fieldset>
  );
}

/** Match modes for a vocabulary/author set condition. */
export type SetMode = "all" | "any" | "none";

const MODE_WORD: Record<SetMode, string> = { all: "all of", any: "any of", none: "none of" };

/** Vocabulary families edited through the mode-select + typeahead widget. */
export type VocabFamily = "authors" | "tags" | "genres" | "moods";

const VOCAB_KIND: Record<VocabFamily, SuggestKind> = {
  authors: "authors",
  tags: "tags",
  genres: "genres",
  moods: "moods",
};

function initialMode(set: SetFilter): SetMode {
  if (set.any.length > 0) return "any";
  if (set.all.length > 0) return "all";
  if (set.none.length > 0) return "none";
  return "any";
}

function isSetMode(value: string): value is SetMode {
  return value === "all" || value === "any" || value === "none";
}

type VocabEditorProps = {
  family: VocabFamily;
  draft: FilterState;
  setDraft: (next: FilterState) => void;
  resolveAuthorLabel: (id: string) => string;
};

export function VocabEditor({
  family,
  draft,
  setDraft,
  resolveAuthorLabel,
}: Readonly<VocabEditorProps>): ReactElement {
  const set = draft[family];
  const [mode, setMode] = useState<SetMode>(() => initialMode(set));
  const isAuthors = family === "authors";

  const selected: TypeaheadOption[] = set[mode].map((token) =>
    isAuthors ? { id: token, value: resolveAuthorLabel(token) } : { value: token },
  );

  function handleChange(next: TypeaheadOption[]): void {
    // Authors store the id (uuid) as the URL token; other vocabularies store
    // the display value, which is the token itself.
    const tokens = next.map((option) => (isAuthors ? (option.id ?? option.value) : option.value));
    // A token lives in at most one mode: writing it to the active mode drops
    // it from the other two, so the same author cannot sit in both `any` and
    // `none` at once.
    const updated: SetFilter = {
      all: mode === "all" ? tokens : set.all.filter((token) => !tokens.includes(token)),
      any: mode === "any" ? tokens : set.any.filter((token) => !tokens.includes(token)),
      none: mode === "none" ? tokens : set.none.filter((token) => !tokens.includes(token)),
    };
    setDraft({ ...draft, [family]: updated });
  }

  return (
    <div className="flex flex-col gap-2">
      <Select
        value={mode}
        onValueChange={(value) => {
          if (isSetMode(value)) setMode(value);
        }}
      >
        <SelectTrigger className="w-32" aria-label="Match mode">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="any">any of</SelectItem>
          <SelectItem value="all">all of</SelectItem>
          <SelectItem value="none">none of</SelectItem>
        </SelectContent>
      </Select>
      <TypeaheadMultiSelect
        kind={VOCAB_KIND[family]}
        label={`${family} (${MODE_WORD[mode]})`}
        selected={selected}
        onChange={handleChange}
      />
    </div>
  );
}
