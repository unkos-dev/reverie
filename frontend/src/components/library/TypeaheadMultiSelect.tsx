/**
 * Controlled vocabulary selection over server-ranked suggestions without local text filtering.
 * Callers own persisted selections; author identity uses UUIDs while other vocabularies use labels.
 */
import { useQuery } from "@tanstack/react-query";
import { useState, type ReactElement } from "react";
import {
  Button,
  ComboBox,
  ComboBoxValue,
  Input,
  Label,
  ListBox,
  ListBoxItem,
  Popover,
  Tag,
  TagGroup,
  TagList,
  type Key,
} from "react-aria-components";
import { X } from "lucide-react";

import { suggestVocab, type SuggestKind } from "@/api";
import { queryKeys } from "@/lib/query/keys";

/** A display label with an author UUID when available; other vocabularies use the label as identity. */
export type TypeaheadOption = { id?: string; value: string };

type Props = {
  kind: SuggestKind;
  label: string;
  selected: readonly TypeaheadOption[];
  onChange: (next: TypeaheadOption[]) => void;
  disabled?: boolean;
};

/** Emits the complete selection on add/remove and clears the search query after an option is committed. */
export function TypeaheadMultiSelect({
  kind,
  label,
  selected,
  onChange,
  disabled = false,
}: Readonly<Props>): ReactElement {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const trimmed = query.trim();
  const { data, isFetching, isError } = useQuery({
    queryKey: queryKeys.suggest.vocab(kind, trimmed),
    queryFn: ({ signal }) => suggestVocab(kind, trimmed, signal),
    enabled: open && !disabled && trimmed.length >= 2,
    staleTime: 30_000,
  });
  function keyFor(option: TypeaheadOption): string {
    return kind === "authors" ? (option.id ?? option.value) : option.value;
  }
  const suggestions =
    trimmed.length >= 2
      ? (data ?? []).map((row) => ({ id: row.id ?? undefined, value: row.value }))
      : [];
  const optionsByKey = new Map<Key, TypeaheadOption>(
    [...suggestions, ...selected].map((item) => [keyFor(item), item]),
  );
  function changeSelection(keys: Key[]): void {
    onChange(
      keys.flatMap((key) => {
        const item = optionsByKey.get(key);
        return item ? [item] : [];
      }),
    );
    setQuery("");
  }
  let status: string | null = null;
  if (trimmed.length < 2) status = "Type at least 2 characters to search.";
  else if (isError) status = "Couldn't load suggestions. Try again.";
  else if (isFetching && suggestions.length === 0) status = "Searching…";
  else if (suggestions.length === 0) status = "No matches.";

  return (
    <ComboBox<TypeaheadOption, "multiple">
      selectionMode="multiple"
      items={suggestions}
      inputValue={query}
      onInputChange={setQuery}
      value={selected.map(keyFor)}
      onChange={changeSelection}
      onOpenChange={setOpen}
      allowsEmptyCollection
      isDisabled={disabled}
      className="flex flex-col gap-2"
    >
      <Label className="sr-only">Add {label}</Label>
      <div className="flex items-center gap-2 rounded-lg border border-input px-2.5 dark:bg-input/30">
        <Input
          placeholder={`Add ${label}…`}
          className="h-8 min-w-0 flex-1 bg-transparent text-sm outline-none disabled:opacity-50"
        />
        <Button
          aria-label={`Show ${label} suggestions`}
          className="rounded-sm px-1 text-muted-foreground"
        >
          ⌄
        </Button>
      </div>
      <ComboBoxValue<TypeaheadOption>>
        <TagGroup
          aria-label={`Selected ${label}`}
          disabledKeys={disabled ? selected.map(keyFor) : []}
          onRemove={(keys) => {
            onChange(selected.filter((item) => !keys.has(keyFor(item))));
          }}
        >
          <TagList items={selected} className="flex flex-wrap gap-1.5">
            {(item) => (
              <Tag
                id={keyFor(item)}
                textValue={item.value}
                className="border-border bg-accent-soft text-fg inline-flex min-h-6 items-center gap-1 rounded-full px-2.5 py-0.5 font-mono text-xs outline-none data-focus-visible:ring-2 data-focus-visible:ring-ring"
              >
                {item.value}
                <Button
                  slot="remove"
                  className="flex min-h-6 min-w-6 items-center justify-center rounded-full"
                >
                  <X className="size-3" aria-hidden="true" />
                </Button>
              </Tag>
            )}
          </TagList>
        </TagGroup>
      </ComboBoxValue>
      <Popover className="z-50 w-(--trigger-width) max-w-[calc(100vw-2rem)] rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10">
        {status && (
          <p role="status" className="px-2 py-3 text-sm text-muted-foreground">
            {status}
          </p>
        )}
        <ListBox<TypeaheadOption>
          aria-label={`${label} suggestions`}
          className="max-h-60 overflow-auto outline-none"
        >
          {(item) => (
            <ListBoxItem
              id={keyFor(item)}
              textValue={item.value}
              className="cursor-default rounded-md px-2 py-1.5 text-sm outline-none data-focused:bg-muted data-selected:font-semibold"
            >
              {item.value}
            </ListBoxItem>
          )}
        </ListBox>
      </Popover>
    </ComboBox>
  );
}
