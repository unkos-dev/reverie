/**
 * Per-column filter editors for the library filter builder. Each is a
 * controlled presentational form: it reads its slice of the filter state and
 * reports edits through `onChange`, holding no state the builder does not own
 * (the text operator is the one exception, kept locally so a chosen operator
 * survives an empty value).
 */
import { type ReactElement, useState } from "react";

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
import type { RangeFilter, TextFilter } from "@/routes/library-params";

/** A text-column comparator. `empty` filters on the column being null. */
export type TextOp = "contains" | "eq" | "ne" | "empty";

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

function isTextOp(value: string): value is TextOp {
  return value === "contains" || value === "eq" || value === "ne" || value === "empty";
}

type TextFilterEditorProps = {
  value: TextFilter;
  ops: readonly TextOp[];
  onChange: (next: TextFilter) => void;
};

export function TextFilterEditor({ value, ops, onChange }: TextFilterEditorProps): ReactElement {
  const [op, setOp] = useState<TextOp>(() => deriveTextOp(value, ops));
  const text = value.contains ?? value.eq ?? value.ne ?? "";

  function changeOp(next: TextOp): void {
    setOp(next);
    onChange(textFilterFor(next, text));
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-1">
        <Label htmlFor="text-op">Operator</Label>
        <Select
          value={op}
          onValueChange={(next) => {
            if (isTextOp(next)) changeOp(next);
          }}
        >
          <SelectTrigger id="text-op" aria-label="Operator">
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
          <Label htmlFor="text-value">Value</Label>
          <Input
            id="text-value"
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
  const isEmpty = value.empty === true;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-end gap-2">
        <div className="flex flex-1 flex-col gap-1">
          <Label htmlFor="range-min">Min</Label>
          <Input
            id="range-min"
            type="number"
            min={min}
            max={max}
            disabled={isEmpty}
            value={value.gte === undefined ? "" : String(value.gte)}
            onChange={(event) => {
              onChange({ ...value, gte: parseIntOrUndefined(event.target.value) });
            }}
          />
        </div>
        <div className="flex flex-1 flex-col gap-1">
          <Label htmlFor="range-max">Max</Label>
          <Input
            id="range-max"
            type="number"
            min={min}
            max={max}
            disabled={isEmpty}
            value={value.lte === undefined ? "" : String(value.lte)}
            onChange={(event) => {
              onChange({ ...value, lte: parseIntOrUndefined(event.target.value) });
            }}
          />
        </div>
      </div>
      {allowEmpty ? (
        <div className="flex items-center gap-2">
          <Checkbox
            id="range-empty"
            checked={isEmpty}
            onCheckedChange={(checked) => {
              onChange(checked === true ? { empty: true } : {});
            }}
          />
          <Label htmlFor="range-empty">Has no value</Label>
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
  return (
    <div className="flex items-end gap-2">
      <div className="flex flex-1 flex-col gap-1">
        <Label htmlFor="date-after">After</Label>
        <Input
          id="date-after"
          type="date"
          value={after ?? ""}
          onChange={(event) => {
            onChange({ after: event.target.value || undefined, before });
          }}
        />
      </div>
      <div className="flex flex-1 flex-col gap-1">
        <Label htmlFor="date-before">Before</Label>
        <Input
          id="date-before"
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
            id={`status-${option.token}`}
            checked={value.includes(option.token)}
            onCheckedChange={(checked) => {
              toggle(option.token, checked === true);
            }}
          />
          <Label htmlFor={`status-${option.token}`}>{option.label}</Label>
        </div>
      ))}
    </fieldset>
  );
}
