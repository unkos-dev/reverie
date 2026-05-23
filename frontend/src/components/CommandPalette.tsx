/**
 * Global Cmd-K command palette (11b).
 *
 * Mounted once at the application root (`App.tsx`) so the global
 * `useEffect` keybinding survives route transitions. Opens on
 * `Cmd-K` / `Ctrl-K`; cmdk's built-in `Esc` handling closes it; on
 * select the React Router `navigate(...)` jumps to the chosen book.
 *
 * The internal search input is debounced 200 ms so a fast typer
 * doesn't fire one request per keystroke. The react-query cache
 * keys on the trimmed query — repeating a previous query is free.
 *
 * # Highlighted snippets
 * The backend emits `ts_headline` snippets bracketed by ASCII control
 * codepoints `\x02` STX (start) and `\x03` ETX (end). Those bytes are
 * reserved by Unicode and never appear in valid text, so they don't
 * collide with user-authored typography (e.g. French guillemets
 * `‹›`, math notation). The `<HighlightedSnippet>` helper splits on
 * those bytes and renders `<mark>` runs — no `dangerouslySetInnerHTML`.
 *
 * The marker bytes must stay in lockstep with the
 * `SNIPPET_HL_START` / `SNIPPET_HL_END` constants in
 * `backend/src/routes/library/search.rs`.
 */
import { useQuery } from "@tanstack/react-query";
import { type ReactElement, useEffect, useState } from "react";
import { useNavigate } from "react-router";

import { searchLibrary, type SearchHit } from "@/api";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { queryKeys } from "@/lib/query/keys";

/** Debounce delay between keystroke and the `/api/search` request. */
const DEBOUNCE_MS = 200;
/** Minimum query length before a request fires — avoids one-char noise. */
const MIN_Q_LEN = 2;
/**
 * Snippet highlight markers — must match `SNIPPET_HL_START` /
 * `SNIPPET_HL_END` in `backend/src/routes/library/search.rs`. ASCII
 * STX/ETX are reserved by Unicode and cannot legally appear in
 * UTF-8 text, so they cannot be confused with user typography.
 */
const SNIPPET_HL_START = "\u0002";
const SNIPPET_HL_END = "\u0003";

/**
 * Listen for `Cmd-K` / `Ctrl-K` and toggle the palette open. The
 * dialog itself owns `Esc`-to-close via Radix. Returns a `[open,
 * setOpen]` tuple so the caller can also open the palette via a
 * header button if one ships later.
 */
function useCmdKToggle(): [boolean, (open: boolean) => void] {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      if (event.key !== "k" && event.key !== "K") return;
      if (!event.metaKey && !event.ctrlKey) return;
      event.preventDefault();
      setOpen((current) => !current);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, []);
  return [open, setOpen];
}

/** Debounced echo of `value` — updates `DEBOUNCE_MS` after the last change. */
function useDebounced<T>(value: T, delay: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const handle = window.setTimeout(() => {
      setDebounced(value);
    }, delay);
    return () => {
      window.clearTimeout(handle);
    };
  }, [value, delay]);
  return debounced;
}

/** Global Cmd-K search palette. */
export function CommandPalette(): ReactElement {
  const [open, setOpen] = useCmdKToggle();
  const [query, setQuery] = useState("");
  const debouncedQuery = useDebounced(query.trim(), DEBOUNCE_MS);
  const navigate = useNavigate();

  const { data, isFetching, isError, error } = useQuery({
    queryKey: queryKeys.search(debouncedQuery),
    queryFn: ({ signal }) => searchLibrary(debouncedQuery, signal),
    enabled: open && debouncedQuery.length >= MIN_Q_LEN,
    staleTime: 30_000,
  });

  useEffect(() => {
    if (isError) {
      // Surface the raw error to the console per frontend/CLAUDE.md.
      // The visible <CommandEmpty> below stays user-friendly; the
      // console keeps the operator-actionable detail (ZodError, 5xx
      // status, network failure shape).
      console.error("[CommandPalette] search query failed", error);
    }
  }, [isError, error]);

  function handleSelect(hit: SearchHit): void {
    setOpen(false);
    setQuery("");
    void navigate(`/b/${hit.id}`);
  }

  const items = data?.items ?? [];
  // Currently the backend emits only `kind: "book"`; author/series
  // result kinds land in a follow-up that fans the hybrid CTE over
  // authors.name and series.name trigram indexes. The group split
  // already exists so the UI doesn't need a structural change when
  // those kinds appear.
  const bookHits = items;

  return (
    <CommandDialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setQuery("");
      }}
      title="Search library"
      description="Search by title, description, or author across your library."
    >
      {/* Server is the filter — cmdk's prefix filter is disabled so
          ranked results render in the order the backend chose. */}
      <Command shouldFilter={false}>
        <CommandInput placeholder="Search library…" value={query} onValueChange={setQuery} />
        <CommandList>
          {renderStatus({
            query: debouncedQuery,
            isFetching,
            isError,
            hasResults: items.length > 0,
          })}
          {bookHits.length > 0 ? (
            <CommandGroup heading="Books">
              {bookHits.map((hit) => (
                <CommandItem
                  key={hit.id}
                  value={hit.id}
                  onSelect={() => {
                    handleSelect(hit);
                  }}
                >
                  <BookHitRow hit={hit} />
                </CommandItem>
              ))}
            </CommandGroup>
          ) : null}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}

interface StatusRenderArgs {
  query: string;
  isFetching: boolean;
  isError: boolean;
  hasResults: boolean;
}

function renderStatus({
  query,
  isFetching,
  isError,
  hasResults,
}: StatusRenderArgs): ReactElement | null {
  if (query.length < MIN_Q_LEN) {
    return <CommandEmpty>Type to search your library.</CommandEmpty>;
  }
  if (isError) {
    return <CommandEmpty>Search failed. Try again in a moment.</CommandEmpty>;
  }
  if (isFetching && !hasResults) {
    return <CommandEmpty>Searching…</CommandEmpty>;
  }
  if (!hasResults) {
    return <CommandEmpty>No results.</CommandEmpty>;
  }
  return null;
}

interface BookHitRowProps {
  hit: SearchHit;
}

function BookHitRow({ hit }: BookHitRowProps): ReactElement {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-fg text-sm font-medium">{hit.title}</span>
      {hit.authors.length > 0 ? (
        <span className="text-fg-muted text-xs">{hit.authors.join(", ")}</span>
      ) : null}
      {hit.snippet ? (
        <HighlightedSnippet text={hit.snippet} className="text-fg-muted text-xs leading-snug" />
      ) : null}
    </div>
  );
}

interface HighlightedSnippetProps {
  text: string;
  className?: string;
}

/**
 * Split a `ts_headline` snippet on the STX/ETX markers Postgres
 * emits and render the highlighted runs as `<mark>`. Plain text-node
 * rendering (no `dangerouslySetInnerHTML`) — safe against
 * HTML-bearing titles or descriptions.
 */
export function HighlightedSnippet({ text, className }: HighlightedSnippetProps): ReactElement {
  const parts: { text: string; highlight: boolean; start: number }[] = [];
  let cursor = 0;
  let highlight = false;
  while (cursor < text.length) {
    const marker = highlight ? SNIPPET_HL_END : SNIPPET_HL_START;
    const nextIndex = text.indexOf(marker, cursor);
    if (nextIndex === -1) {
      parts.push({ text: text.slice(cursor), highlight, start: cursor });
      break;
    }
    parts.push({ text: text.slice(cursor, nextIndex), highlight, start: cursor });
    cursor = nextIndex + marker.length;
    highlight = !highlight;
  }
  return (
    <span className={className}>
      {parts.map((part) => {
        // Stable id baked into each split: position in text plus a
        // marker tag, so React's reconciliation never collides even
        // when two runs share identical text.
        const key = `${String(part.start)}-${part.highlight ? "h" : "p"}`;
        return part.highlight ? (
          <mark key={key} className="bg-accent-soft text-fg rounded-sm px-0.5">
            {part.text}
          </mark>
        ) : (
          <span key={key}>{part.text}</span>
        );
      })}
    </span>
  );
}
