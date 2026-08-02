/**
 * Per-column filter editors for the library filter builder. Each is a
 * controlled presentational form: it reads its slice of the filter state and
 * reports edits through `onChange`, holding no state the builder does not own
 * (the text operator is the one exception, kept locally so a chosen operator
 * survives an empty value).
 */
import { type ReactElement, useId, useState } from "react";

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
  const text = value.contains ?? value.eq ?? value.ne ?? "";

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
              onChange(textFilterFor(op, event.target.value));
            }}
          />
        </div>
      )}
    </div>
  );
}

/** Strict integer parse: non-integer or empty input yields `undefined`. */
function parseIntOrUndefined(raw: string): number | undefined {
  return /^-?\d+$/.test(raw) ? Number(raw) : undefined;
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
      <div className="flex items-end gap-2">
        <div className="flex flex-1 flex-col gap-1">
          <Label htmlFor={`${id}-min`}>Min</Label>
          <Input
            id={`${id}-min`}
            type="number"
            min={min}
            max={max}
            disabled={isEmpty}
            value={value.gte === undefined ? "" : String(value.gte)}
            onChange={(event) => {
              onChange({
                ...value,
                gte: parseIntOrUndefined(event.target.value),
              });
            }}
          />
        </div>
        <div className="flex flex-1 flex-col gap-1">
          <Label htmlFor={`${id}-max`}>Max</Label>
          <Input
            id={`${id}-max`}
            type="number"
            min={min}
            max={max}
            disabled={isEmpty}
            value={value.lte === undefined ? "" : String(value.lte)}
            onChange={(event) => {
              onChange({
                ...value,
                lte: parseIntOrUndefined(event.target.value),
              });
            }}
          />
        </div>
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
    <div className="flex items-end gap-2">
      <div className="flex flex-1 flex-col gap-1">
        <Label htmlFor={`${id}-after`}>After</Label>
        <Input
          id={`${id}-after`}
          type="date"
          value={after ?? ""}
          onChange={(event) => {
            onChange({ after: event.target.value || undefined, before });
          }}
        />
      </div>
      <div className="flex flex-1 flex-col gap-1">
        <Label htmlFor={`${id}-before`}>Before</Label>
        <Input
          id={`${id}-before`}
          type="date"
          value={before ?? ""}
          onChange={(event) => {
            onChange({ after, before: event.target.value || undefined });
          }}
        />
      </div>
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

const MODE_WORD: Record<SetMode, string> = {
  all: "all of",
  any: "any of",
  none: "none of",
};

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
