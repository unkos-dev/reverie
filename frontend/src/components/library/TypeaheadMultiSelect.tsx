/**
 * Controlled vocabulary selection over server-ranked suggestions without local text filtering.
 * Callers own persisted selections; author and series identity use GraphQL IDs while other vocabularies use labels.
 */
import { fetchQuery, useRelayEnvironment } from "react-relay";
import type { Observable } from "relay-runtime";
import {
  readerAuthorSuggestionsQuery,
  readerSeriesSuggestionsQuery,
  readerGenreSuggestionsQuery,
  readerMoodSuggestionsQuery,
  readerTagSuggestionsQuery,
} from "@/lib/relay/reader";
import type { readerAuthorSuggestionsQuery as AuthorQuery } from "@/lib/relay/__generated__/readerAuthorSuggestionsQuery.graphql";
import type { readerSeriesSuggestionsQuery as SeriesQuery } from "@/lib/relay/__generated__/readerSeriesSuggestionsQuery.graphql";
import type { readerGenreSuggestionsQuery as GenreQuery } from "@/lib/relay/__generated__/readerGenreSuggestionsQuery.graphql";
import type { readerMoodSuggestionsQuery as MoodQuery } from "@/lib/relay/__generated__/readerMoodSuggestionsQuery.graphql";
import type { readerTagSuggestionsQuery as TagQuery } from "@/lib/relay/__generated__/readerTagSuggestionsQuery.graphql";
import { useEffect, useState, type ReactElement } from "react";
import {
  Button,
  ComboBox,
  ComboBoxValue,
  FieldError,
  Input,
  Label,
  ListBox,
  ListBoxItem,
  Popover,
  Tag,
  TagGroup,
  TagList,
  Text,
  type Key,
} from "react-aria-components";
import { X } from "lucide-react";

import { type SuggestKind } from "@/api";

/** A display label with an author or series GraphQL ID; other vocabularies use the label as identity. */
export type TypeaheadOption = { id?: string; value: string };

type Props = {
  kind: SuggestKind;
  label: string;
  selected: readonly TypeaheadOption[];
  onChange: (next: TypeaheadOption[]) => void;
  disabled?: boolean;
  maxSelectedItems?: number;
};

/** Emits the complete native selection on add/remove. */
export function TypeaheadMultiSelect({
  kind,
  label,
  selected,
  onChange,
  disabled = false,
  maxSelectedItems,
}: Readonly<Props>): ReactElement {
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const trimmed = query.trim();
  const environment = useRelayEnvironment();
  const enabled = open && !disabled && trimmed.length >= 2;
  const token = JSON.stringify([kind, trimmed]);
  const [result, setResult] = useState<{
    token: string;
    data?: readonly { entityId: string | null | undefined; value: string }[];
    error?: Error;
  }>();
  useEffect(() => {
    if (!enabled) return;
    const variables = { q: trimmed };
    let request: Observable<readonly { entityId: string | null | undefined; value: string }[]>;
    if (kind === "authors")
      request = fetchQuery<AuthorQuery>(environment, readerAuthorSuggestionsQuery, variables).map(
        (data) => data.authorSuggestions,
      );
    else if (kind === "series")
      request = fetchQuery<SeriesQuery>(environment, readerSeriesSuggestionsQuery, variables).map(
        (data) => data.seriesSuggestions,
      );
    else if (kind === "genres")
      request = fetchQuery<GenreQuery>(environment, readerGenreSuggestionsQuery, variables).map(
        (data) => data.genreSuggestions,
      );
    else if (kind === "moods")
      request = fetchQuery<MoodQuery>(environment, readerMoodSuggestionsQuery, variables).map(
        (data) => data.moodSuggestions,
      );
    else
      request = fetchQuery<TagQuery>(environment, readerTagSuggestionsQuery, variables).map(
        (data) => data.tagSuggestions,
      );

    const subscription = request.subscribe({
      next: (data) => {
        setResult({ token, data });
      },
      error: (error: Error) => {
        console.error("[TypeaheadMultiSelect] suggestions failed", error);
        setResult({ token, error });
      },
    });
    return () => {
      subscription.unsubscribe();
    };
  }, [environment, enabled, kind, trimmed, token]);
  const data = result?.token === token ? result.data : undefined;
  const isError = result?.token === token && result.error !== undefined;
  const isFetching = enabled && data === undefined && !isError;
  function keyFor(option: TypeaheadOption): string {
    return kind === "authors" || kind === "series" ? (option.id ?? option.value) : option.value;
  }
  const suggestions =
    trimmed.length >= 2
      ? (data ?? []).map((row) => {
          const id =
            row.entityId != null && (kind === "authors" || kind === "series")
              ? row.entityId
              : undefined;
          return { id, value: row.value };
        })
      : [];
  const optionsByKey = new Map<Key, TypeaheadOption>(
    [...suggestions, ...selected].map((item) => [keyFor(item), item]),
  );
  const selectedKeys = selected.map(keyFor);
  const selectedValues = selected.map((item) => item.value).join(", ");
  const selectionLabel =
    selected.length === 0 ? `Selected ${label}` : `Selected ${label}: ${selectedValues}`;
  const atLimit = maxSelectedItems !== undefined && selected.length >= maxSelectedItems;
  const overLimit = maxSelectedItems !== undefined && selected.length > maxSelectedItems;
  const disabledKeys = atLimit
    ? suggestions.map(keyFor).filter((key) => !selectedKeys.includes(key))
    : [];
  function changeSelection(keys: Key[]): void {
    onChange(
      keys.flatMap((key) => {
        const item = optionsByKey.get(key);
        return item ? [item] : [];
      }),
    );
    if (kind !== "series") setQuery("");
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
      value={selectedKeys}
      disabledKeys={disabledKeys}
      onChange={changeSelection}
      onOpenChange={setOpen}
      allowsEmptyCollection
      allowsCustomValue={kind === "series"}
      isDisabled={disabled}
      isInvalid={overLimit}
      validationBehavior="aria"
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
          aria-label={selectionLabel}
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
      <Text slot="description" className="text-sm text-muted-foreground empty:hidden">
        {atLimit && !overLimit && <>{maxSelectedItems} selected. Remove one to add another.</>}
      </Text>
      <FieldError className="text-fg-muted text-sm">
        Choose {maxSelectedItems} or fewer values.
      </FieldError>
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
              className="cursor-default rounded-md px-2 py-1.5 text-sm outline-none data-focused:bg-muted data-selected:font-semibold data-disabled:opacity-50"
            >
              {item.value}
            </ListBoxItem>
          )}
        </ListBox>
      </Popover>
    </ComboBox>
  );
}
