import { type ReactElement, useId, useMemo, useState } from "react";
import { parseDate } from "@internationalized/date";
import { CalendarDays, ChevronLeft, ChevronRight, Minus, Plus } from "lucide-react";
import {
  Calendar,
  CalendarCell,
  CalendarGrid,
  CalendarGridBody,
  CalendarGridHeader,
  CalendarHeaderCell,
  DateInput,
  DatePicker,
  DateSegment,
  Dialog,
  FieldError,
  Group,
  Heading,
  Input as AriaInput,
  Label as AriaLabel,
  NumberField,
  Popover,
  TextField,
} from "react-aria-components";

import type { SuggestKind } from "@/api";
import { Checkbox, CheckboxGroup } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
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
  type StatusFilter,
} from "@/routes/library-params";

import { TypeaheadMultiSelect, type TypeaheadOption } from "./TypeaheadMultiSelect";

export type { TextOp } from "@/routes/library-params";

const TEXT_OP_LABELS: Record<TextOp, string> = {
  contains: "contains",
  eq: "equals",
  ne: "does not equal",
  empty: "is empty",
};

type TextFilterEditorProps = {
  value: TextFilter;
  ops: readonly TextOp[];
  onChange: (next: TextFilter) => void;
};

export function TextFilterEditor({
  value,
  ops,
  onChange,
}: Readonly<TextFilterEditorProps>): ReactElement {
  const id = useId();
  const op = value.op ?? ops[0];
  const text = value.text ?? "";

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-1">
        <Label htmlFor={`${id}-op`}>Operator</Label>
        <Select
          aria-label="Operator"
          value={op}
          onChange={(next) => {
            if (typeof next !== "string") return;
            if (isTextOp(next)) onChange({ op: next, text });
          }}
        >
          <SelectTrigger id={`${id}-op`}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {ops.map((candidate) => (
              <SelectItem key={candidate} id={candidate}>
                {TEXT_OP_LABELS[candidate]}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
      {op === "empty" ? null : (
        <TextField
          value={text}
          onChange={(next) => {
            onChange({ op, text: next });
          }}
          className="flex flex-col gap-1"
        >
          <AriaLabel aria-label="Filter value" className="text-sm font-medium">
            Value
          </AriaLabel>
          <AriaInput render={(props) => <Input {...props} />} />
        </TextField>
      )}
    </div>
  );
}

type RangeFilterEditorProps = {
  label?: string;
  value: RangeFilter;
  min?: number;
  max?: number;
  allowEmpty?: boolean;
  showContradiction?: boolean;
  onChange: (patch: (current: RangeFilter) => RangeFilter) => void;
};

export function RangeFilterEditor({
  label = "Range",
  value,
  min,
  max,
  allowEmpty = false,
  showContradiction = true,
  onChange,
}: Readonly<RangeFilterEditorProps>): ReactElement {
  const id = useId();
  const contradictory =
    showContradiction &&
    value.gte !== undefined &&
    value.lte !== undefined &&
    value.gte > value.lte;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-3" aria-describedby={contradictory ? id : undefined}>
        {(["gte", "lte"] as const).map((side) => (
          <NumberField
            key={side}
            aria-describedby={contradictory ? id : undefined}
            value={value[side] ?? NaN}
            minValue={min}
            maxValue={max}
            step={1}
            isDisabled={value.empty === true}
            onChange={(next) => {
              onChange((current) => ({
                ...current,
                [side]: Number.isNaN(next) ? undefined : next,
              }));
            }}
            className="flex min-w-0 flex-col gap-1"
          >
            <AriaLabel
              aria-label={`${label} ${side === "gte" ? "minimum" : "maximum"}`}
              className="text-sm font-medium"
            >
              {side === "gte" ? "Minimum" : "Maximum"}
            </AriaLabel>
            <Group className="border-input bg-transparent dark:bg-input/30 flex h-9 min-w-0 items-center rounded-lg border px-0.5 data-disabled:opacity-50">
              <AriaInput className="text-fg min-w-0 flex-1 bg-transparent px-2 text-sm tabular-nums outline-none" />
              <Button slot="decrement" variant="ghost" size="icon">
                <Minus aria-hidden="true" />
              </Button>
              <Button slot="increment" variant="ghost" size="icon">
                <Plus aria-hidden="true" />
              </Button>
            </Group>
            <FieldError className="text-fg-muted text-sm" />
          </NumberField>
        ))}
      </div>
      {allowEmpty ? (
        <Checkbox
          isSelected={value.empty === true}
          onChange={(selected) => {
            onChange(() => (selected ? { empty: true } : {}));
          }}
        >
          Has no value
        </Checkbox>
      ) : null}
      {contradictory ? (
        <p id={id} className="text-fg-muted text-sm">
          Minimum is above maximum. No books match this range.
        </p>
      ) : null}
    </div>
  );
}

type DateRangeEditorProps = {
  after?: string;
  before?: string;
  showContradiction?: boolean;
  onChange: (
    patch: (current: { after?: string; before?: string }) => { after?: string; before?: string },
  ) => void;
};

export function DateRangeEditor({
  after,
  before,
  showContradiction = true,
  onChange,
}: Readonly<DateRangeEditorProps>): ReactElement {
  const id = useId();
  const afterValue = useMemo(() => (after === undefined ? null : parseDate(after)), [after]);
  const beforeValue = useMemo(() => (before === undefined ? null : parseDate(before)), [before]);
  const contradictory =
    showContradiction &&
    after !== undefined &&
    before !== undefined &&
    parseDate(after).compare(parseDate(before)) > 0;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-col gap-3">
        {(["after", "before"] as const).map((side) => {
          const value = side === "after" ? afterValue : beforeValue;
          const label = side === "after" ? "On or after" : "On or before";
          return (
            <DatePicker
              key={side}
              aria-describedby={contradictory ? id : undefined}
              value={value}
              onChange={(date) => {
                onChange((current) =>
                  side === "after"
                    ? { ...current, after: date?.toString() }
                    : { ...current, before: date?.toString() },
                );
              }}
              className="flex min-w-0 flex-col gap-1"
            >
              <AriaLabel
                aria-label={`Added ${label.toLowerCase()}`}
                className="text-sm font-medium"
              >
                {label}
              </AriaLabel>
              <Group className="border-input bg-transparent dark:bg-input/30 flex h-9 min-w-0 items-center rounded-lg border px-0.5">
                <DateInput className="flex min-w-0 flex-1 items-center px-2 text-sm">
                  {(segment) => (
                    <DateSegment
                      segment={segment}
                      className="rounded-sm px-0.5 tabular-nums data-placeholder:text-fg-muted data-focused:bg-accent-soft data-focused:text-fg"
                    />
                  )}
                </DateInput>
                <Button variant="ghost" size="icon">
                  <CalendarDays aria-hidden="true" />
                </Button>
              </Group>
              <FieldError className="text-fg-muted text-sm" />
              <Popover
                className="bg-surface text-fg border-border z-50 w-[280px] max-w-[calc(100vw-16px)] rounded-lg border p-2"
                offset={8}
              >
                <Dialog className="outline-none">
                  <Calendar>
                    <header className="mb-2 flex items-center justify-between gap-2">
                      <Button slot="previous" variant="ghost" size="icon">
                        <ChevronLeft aria-hidden="true" />
                      </Button>
                      <Heading className="text-sm font-medium" />
                      <Button slot="next" variant="ghost" size="icon">
                        <ChevronRight aria-hidden="true" />
                      </Button>
                    </header>
                    <CalendarGrid className="w-full border-separate border-spacing-0">
                      <CalendarGridHeader>
                        {(day) => (
                          <CalendarHeaderCell className="text-fg-muted pb-1 text-xs font-normal">
                            {day}
                          </CalendarHeaderCell>
                        )}
                      </CalendarGridHeader>
                      <CalendarGridBody>
                        {(date) => (
                          <CalendarCell
                            date={date}
                            className="text-fg mx-auto flex size-9 items-center justify-center rounded-md text-sm tabular-nums outline-none data-hovered:bg-surface-2 data-selected:bg-accent-soft data-selected:font-semibold data-outside-month:text-fg-muted data-disabled:opacity-50 data-focus-visible:outline-2 data-focus-visible:outline-focus-ring"
                          />
                        )}
                      </CalendarGridBody>
                    </CalendarGrid>
                  </Calendar>
                </Dialog>
              </Popover>
            </DatePicker>
          );
        })}
      </div>
      {contradictory ? (
        <p id={id} className="text-fg-muted text-sm">
          Start date is after end date. No books match this range.
        </p>
      ) : null}
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
  value: StatusFilter["any"];
  onChange: (next: StatusFilter["any"]) => void;
};

export function StatusEditor({ value, onChange }: Readonly<StatusEditorProps>): ReactElement {
  return (
    <CheckboxGroup
      aria-label="Reading status"
      value={[...value]}
      onChange={(next) => {
        onChange(
          next.filter((token): token is StatusFilter["any"][number] =>
            STATUS_OPTIONS.some((option) => option.token === token),
          ),
        );
      }}
      className="flex flex-col gap-2"
    >
      {STATUS_OPTIONS.map((option) => (
        <Checkbox key={option.token} value={option.token}>
          {option.label}
        </Checkbox>
      ))}
    </CheckboxGroup>
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
        aria-label="Match mode"
        value={mode}
        onChange={(value) => {
          if (typeof value !== "string") return;
          if (isSetMode(value)) setMode(value);
        }}
      >
        <SelectTrigger className="w-32">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem id="any">any of</SelectItem>
          <SelectItem id="all">all of</SelectItem>
          <SelectItem id="none">none of</SelectItem>
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
