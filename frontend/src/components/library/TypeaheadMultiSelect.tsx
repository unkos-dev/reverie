/**
 * Controlled vocabulary selection over server-ranked suggestions without local text filtering.
 * Callers own persisted selections; author and series identity use GraphQL IDs while other vocabularies use labels.
 */
import { fetchQuery, graphql, useRefetchableFragment, useRelayEnvironment } from "react-relay";
import type { Subscription } from "relay-runtime";
import type { TypeaheadMultiSelect_suggestions$key } from "@/lib/relay/__generated__/TypeaheadMultiSelect_suggestions.graphql";
import TypeaheadMultiSelectRefetchQuery, {
  type TypeaheadMultiSelectRefetchQuery as RefetchQuery,
} from "@/lib/relay/__generated__/TypeaheadMultiSelectRefetchQuery.graphql";
import { useEffect, useRef, useState, type ReactElement } from "react";
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

const suggestionsFragment = graphql`
  fragment TypeaheadMultiSelect_suggestions on Query
  @refetchable(queryName: "TypeaheadMultiSelectRefetchQuery")
  @argumentDefinitions(
    q: { type: "String", defaultValue: "" }
    authors: { type: "Boolean", defaultValue: false }
    series: { type: "Boolean", defaultValue: false }
    genres: { type: "Boolean", defaultValue: false }
    moods: { type: "Boolean", defaultValue: false }
    tags: { type: "Boolean", defaultValue: false }
  )
  @throwOnFieldError {
    authorSuggestions(q: $q) @include(if: $authors) {
      id
      name
    }
    seriesSuggestions(q: $q) @include(if: $series) {
      id
      name
    }
    genreSuggestions(q: $q) @include(if: $genres) {
      id
      name
    }
    moodSuggestions(q: $q) @include(if: $moods) {
      id
      name
    }
    tagSuggestions(q: $q) @include(if: $tags) {
      id
      name
    }
  }
`;

const MAX_TERM_CODE_POINTS = 100;

/** A display label with an author or series GraphQL ID; other vocabularies use the label as identity. */
export type TypeaheadOption = { id?: string; value: string };

type Props = {
  suggestions: TypeaheadMultiSelect_suggestions$key;
  kind: SuggestKind;
  label: string;
  selected: readonly TypeaheadOption[];
  onChange: (next: TypeaheadOption[]) => void;
  disabled?: boolean;
  maxSelectedItems?: number;
};

/** Emits the complete native selection on add/remove. */
export function TypeaheadMultiSelect({
  suggestions: suggestionsKey,
  kind,
  label,
  selected,
  onChange,
  disabled = false,
  maxSelectedItems,
}: Readonly<Props>): ReactElement {
  const environment = useRelayEnvironment();
  const [data, refetch] = useRefetchableFragment<
    RefetchQuery,
    TypeaheadMultiSelect_suggestions$key
  >(suggestionsFragment, suggestionsKey);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState<{ term: string; done: boolean; error?: Error }>();
  const subscription = useRef<Subscription | null>(null);
  useEffect(
    () => () => {
      subscription.current?.unsubscribe();
    },
    [],
  );
  const trimmed = query.trim();
  const termTooLong = Array.from(trimmed).length > MAX_TERM_CODE_POINTS;

  function searchFor(text: string, isOpen: boolean): void {
    subscription.current?.unsubscribe();
    subscription.current = null;
    const term = text.trim();
    if (!isOpen || disabled || term.length < 2 || Array.from(term).length > MAX_TERM_CODE_POINTS) {
      setSearch(undefined);
      return;
    }
    const variables = {
      q: term,
      authors: kind === "authors",
      series: kind === "series",
      genres: kind === "genres",
      moods: kind === "moods",
      tags: kind === "tags",
    };
    setSearch({ term, done: false });
    subscription.current = fetchQuery<RefetchQuery>(
      environment,
      TypeaheadMultiSelectRefetchQuery,
      variables,
    ).subscribe({
      complete: () => {
        subscription.current = null;
        refetch(variables, { fetchPolicy: "store-only" });
        setSearch({ term, done: true });
      },
      error: (error: Error) => {
        subscription.current = null;
        console.error("[TypeaheadMultiSelect] suggestions failed", error);
        setSearch({ term, done: true, error });
      },
    });
  }

  const current = search?.term === trimmed ? search : undefined;
  const isError = termTooLong || current?.error !== undefined;
  const isFetching = current !== undefined && !current.done;
  let rows: readonly { id: string; name: string }[] | undefined;
  if (kind === "authors") rows = data.authorSuggestions;
  else if (kind === "series") rows = data.seriesSuggestions;
  else if (kind === "genres") rows = data.genreSuggestions;
  else if (kind === "moods") rows = data.moodSuggestions;
  else rows = data.tagSuggestions;
  function keyFor(option: TypeaheadOption): string {
    return kind === "authors" || kind === "series" ? (option.id ?? option.value) : option.value;
  }
  const suggestions =
    trimmed.length >= 2 && current?.done === true && current.error === undefined
      ? (rows ?? []).map((row) => ({
          id: kind === "authors" || kind === "series" ? row.id : undefined,
          value: row.name,
        }))
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
    if (kind !== "series") {
      setQuery("");
      searchFor("", open);
    }
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
      onInputChange={(text) => {
        setQuery(text);
        searchFor(text, open);
      }}
      value={selectedKeys}
      disabledKeys={disabledKeys}
      onChange={changeSelection}
      onOpenChange={(isOpen) => {
        setOpen(isOpen);
        searchFor(query, isOpen);
      }}
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
