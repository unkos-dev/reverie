/**
 * Accessible multi-select typeahead for the library filter builder.
 *
 * Per the filter-grammar ADR's decomposition (no combobox-with-multiselect
 * pattern is published in APG, and cmdk documents none), this is two in-pattern
 * widgets rather than one invented composite:
 *
 * 1. An APG editable single-select combobox (cmdk inside a Popover). Each Enter
 *    commits exactly one option, the committed option leaves the suggestion
 *    list, the input clears, and the popup stays open for the next pick. No
 *    `aria-selected` persists across picks; the listbox is single-select per
 *    interaction.
 * 2. A separate chip list, adjacent to (not inside) the combobox: a plain list
 *    of labeled dismiss buttons with fully specified button semantics and no
 *    novel ARIA. Backspace in an empty input removes the last chip as a
 *    convenience, never as the only removal path.
 *
 * The component is vocabulary-agnostic: it manages a list of `{ id?, value }`
 * options and leaves the caller to decide which field becomes the URL token
 * (author filters send the id, vocabulary filters send the value).
 */
import { useQuery } from "@tanstack/react-query";
import { X } from "lucide-react";
import { type KeyboardEvent, type ReactElement, useState } from "react";

import { suggestVocab, type SuggestKind } from "@/api";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Popover, PopoverAnchor, PopoverContent } from "@/components/ui/popover";
import { useDebounced } from "@/lib/hooks/use-debounced";
import { queryKeys } from "@/lib/query/keys";

/** Debounce between a keystroke and the suggest request, long enough that a
 *  fast typist fires one request instead of one per key. */
const DEBOUNCE_MS = 250;
/** Minimum query length before a request fires (pg_trgm needs 2+ characters). */
const MIN_Q_LEN = 2;

/** One selectable option. `id` is the vocabulary row id when known; `value` is
 *  the display label and, for vocabulary filters, the URL token itself. */
export type TypeaheadOption = { id?: string; value: string };

type Props = {
  kind: SuggestKind;
  /** Accessible context for the input and chip list, e.g. "author (any of)". */
  label: string;
  selected: readonly TypeaheadOption[];
  onChange: (next: TypeaheadOption[]) => void;
  disabled?: boolean;
};

/**
 * Two options are the same when their ids match (both known), else when their
 * display values match. Id-first keeps two same-named authors distinct; the
 * value fallback lets a chip hydrated from a URL (id unknown) still match its
 * suggestion so it drops out of the list once selected.
 */
function sameOption(a: TypeaheadOption, b: TypeaheadOption): boolean {
  if (a.id !== undefined && b.id !== undefined) return a.id === b.id;
  return a.value === b.value;
}

/** Stable React key / listbox id fragment for an option. */
function optionKey(option: TypeaheadOption): string {
  return option.id ?? option.value;
}

export function TypeaheadMultiSelect({
  kind,
  label,
  selected,
  onChange,
  disabled = false,
}: Props): ReactElement {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const debounced = useDebounced(query.trim(), DEBOUNCE_MS);

  const { data, isFetching, isError } = useQuery({
    queryKey: queryKeys.suggest.vocab(kind, debounced),
    queryFn: ({ signal }) => suggestVocab(kind, debounced, signal),
    enabled: open && debounced.length >= MIN_Q_LEN,
    staleTime: 30_000,
  });

  // Committed options drop out of the suggestion list so the same value cannot
  // be picked twice; the wire null id is normalized to undefined here.
  const suggestions: TypeaheadOption[] = (data ?? [])
    .map((row) => ({ id: row.id ?? undefined, value: row.value }))
    .filter((option) => !selected.some((chosen) => sameOption(chosen, option)));

  function commit(option: TypeaheadOption): void {
    if (selected.some((chosen) => sameOption(chosen, option))) return;
    onChange([...selected, option]);
    // Clear the input but keep the popup open for the next pick; cmdk leaves
    // focus in the input on Enter, so no manual refocus is needed.
    setQuery("");
  }

  function remove(target: TypeaheadOption): void {
    onChange(selected.filter((chosen) => !sameOption(chosen, target)));
  }

  function handleInputKeyDown(event: KeyboardEvent<HTMLInputElement>): void {
    // Backspace on an empty input removes the last chip, alongside (never
    // instead of) the per-chip dismiss buttons.
    if (event.key === "Backspace" && query === "" && selected.length > 0) {
      remove(selected[selected.length - 1]);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {selected.length > 0 ? (
        <ul aria-label={`Selected ${label}`} className="flex flex-wrap gap-1.5">
          {selected.map((chip) => (
            <li key={optionKey(chip)}>
              <button
                type="button"
                disabled={disabled}
                onClick={() => {
                  remove(chip);
                }}
                className="border-border bg-accent-soft text-fg hover:bg-accent-soft/80 inline-flex min-h-6 items-center gap-1 rounded-full px-2.5 py-0.5 font-mono text-xs disabled:opacity-50"
              >
                {chip.value}
                <X className="size-3" aria-hidden="true" />
                <span className="sr-only">Remove {chip.value}</span>
              </button>
            </li>
          ))}
        </ul>
      ) : null}
      {/* cmdk's `label` fills a visually-hidden <label> the input points at via
          aria-labelledby, so the combobox has an accessible name. */}
      <Command shouldFilter={false} label={`Add ${label}`}>
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverAnchor asChild>
            <CommandInput
              placeholder={`Add ${label}…`}
              value={query}
              disabled={disabled}
              onValueChange={(next) => {
                setQuery(next);
                setOpen(true);
              }}
              onFocus={() => {
                setOpen(true);
              }}
              onKeyDown={handleInputKeyDown}
            />
          </PopoverAnchor>
          <PopoverContent
            align="start"
            side="bottom"
            className="w-(--radix-popover-trigger-width) p-0"
            // Keep focus in the input so the user keeps typing while the
            // listbox tracks the active option via aria-activedescendant.
            onOpenAutoFocus={(event) => {
              event.preventDefault();
            }}
          >
            <CommandList>
              <StatusRow
                query={debounced}
                isFetching={isFetching}
                isError={isError}
                hasResults={suggestions.length > 0}
              />
              {suggestions.map((option) => (
                <CommandItem
                  key={optionKey(option)}
                  value={optionKey(option)}
                  onSelect={() => {
                    commit(option);
                  }}
                >
                  {option.value}
                </CommandItem>
              ))}
            </CommandList>
          </PopoverContent>
        </Popover>
      </Command>
    </div>
  );
}

type StatusRowProps = {
  query: string;
  isFetching: boolean;
  isError: boolean;
  hasResults: boolean;
};

/** The listbox's non-result states: below-minimum, error, loading, empty. */
function StatusRow({
  query,
  isFetching,
  isError,
  hasResults,
}: StatusRowProps): ReactElement | null {
  if (query.length < MIN_Q_LEN) {
    return <CommandEmpty>Type at least {MIN_Q_LEN} characters to search.</CommandEmpty>;
  }
  if (isError) {
    return <CommandEmpty>Couldn&apos;t load suggestions. Try again.</CommandEmpty>;
  }
  if (isFetching && !hasResults) {
    return <CommandEmpty>Searching…</CommandEmpty>;
  }
  if (!hasResults) {
    return <CommandEmpty>No matches.</CommandEmpty>;
  }
  return null;
}
