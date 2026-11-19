/**
 * Global library search using server-ranked results and safe snippet highlighting.
 * React Aria owns modal focus and dismissal.
 */
import { graphql, usePreloadedQuery, useQueryLoader, type PreloadedQuery } from "react-relay";
import type { CommandPaletteQuery as SearchQuery } from "@/lib/relay/__generated__/CommandPaletteQuery.graphql";
import { Suspense, useState, type ReactElement } from "react";
import { ErrorBoundary } from "react-error-boundary";
import {
  Autocomplete,
  Button,
  Dialog,
  Heading,
  Input,
  Menu,
  MenuItem,
  Modal,
  ModalOverlay,
  SearchField,
  Text,
} from "react-aria-components";
import { SearchIcon } from "lucide-react";

import type { SearchHit } from "@/api";
type SearchItem = Pick<SearchHit, "id" | "title" | "authors" | "snippet">;

const SNIPPET_HL_START = "\u0002";
const SNIPPET_HL_END = "\u0003";

const commandPaletteQuery = graphql`
  query CommandPaletteQuery($q: String!) @throwOnFieldError {
    search(q: $q) {
      snippet
      manifestation {
        id
        work {
          title
          authors(first: 5) {
            edges {
              node {
                name
              }
            }
            pageInfo {
              hasNextPage
              endCursor
            }
          }
        }
      }
    }
  }
`;

/** Searches from two characters and navigates to the selected book without changing library filters. */
export function CommandPalette({
  isOpen,
  onOpenChange,
}: Readonly<{ isOpen: boolean; onOpenChange: (open: boolean) => void }>): ReactElement {
  const [query, setQuery] = useState("");
  const [reference, loadSearch, disposeSearch] = useQueryLoader<SearchQuery>(commandPaletteQuery);
  const trimmed = query.trim();
  function changeQuery(next: string): void {
    setQuery(next);
    const q = next.trim();
    if (q.length < 2) disposeSearch();
    else if (q !== trimmed || reference == null) loadSearch({ q }, { fetchPolicy: "network-only" });
  }
  function changeOpen(next: boolean): void {
    onOpenChange(next);
    if (!next) {
      setQuery("");
      disposeSearch();
    }
  }
  function close(): void {
    changeOpen(false);
  }

  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={changeOpen}
      isDismissable
      className="fixed inset-0 z-50 flex items-center justify-center bg-overlay p-4"
    >
      <Modal className="w-full max-w-lg overflow-hidden rounded-xl bg-popover p-1 text-popover-foreground ring-1 ring-foreground/10">
        <Dialog className="outline-none">
          <Heading slot="title" className="sr-only">
            Search library
          </Heading>
          <Text slot="description" className="sr-only">
            Search by title, description, or author across your library.
          </Text>
          <Autocomplete inputValue={query} onInputChange={changeQuery}>
            <SearchField
              aria-label="Search library"
              className="flex items-center gap-2 border-b border-border px-3"
            >
              <SearchIcon className="size-4 text-muted-foreground" aria-hidden="true" />
              <Input
                autoFocus
                placeholder="Find a volume by title, author, quote…"
                className="h-10 min-w-0 flex-1 bg-transparent text-sm outline-none"
              />
              <Button aria-label="Clear search" className="rounded-sm p-1 text-muted-foreground">
                ×
              </Button>
            </SearchField>
            {reference == null || trimmed.length < 2 ? (
              <SearchResults status="Type to search your library." onClose={close} />
            ) : (
              <ErrorBoundary
                resetKeys={[reference]}
                onError={(error) => {
                  console.error("[CommandPalette] search failed", error);
                }}
                fallback={
                  <SearchResults status="Search failed. Try again in a moment." onClose={close} />
                }
              >
                <Suspense fallback={<SearchResults status="Searching…" onClose={close} />}>
                  <LoadedSearchResults reference={reference} onClose={close} />
                </Suspense>
              </ErrorBoundary>
            )}
          </Autocomplete>
          <div className="border-border text-fg-muted flex items-center gap-4 border-t px-3 py-2 font-mono text-[0.65rem] tracking-wide">
            <span>
              <kbd>↑↓</kbd> navigate
            </span>
            <span>
              <kbd>↵</kbd> open
            </span>
            <span>
              <kbd>esc</kbd> close
            </span>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}

function LoadedSearchResults({
  reference,
  onClose,
}: Readonly<{ reference: PreloadedQuery<SearchQuery>; onClose: () => void }>): ReactElement {
  const data = usePreloadedQuery<SearchQuery>(commandPaletteQuery, reference);
  const items = data.search.flatMap(({ manifestation, snippet }) =>
    manifestation?.work == null
      ? []
      : [
          {
            id: manifestation.id,
            title: manifestation.work.title,
            authors: manifestation.work.authors.edges.map(({ node }) => node.name),
            snippet: snippet ?? null,
          },
        ],
  );
  return (
    <SearchResults
      status={items.length === 0 ? "No results." : null}
      items={items}
      onClose={onClose}
    />
  );
}

function SearchResults({
  status,
  items = [],
  onClose,
}: Readonly<{ status: string | null; items?: SearchItem[]; onClose: () => void }>): ReactElement {
  return (
    <>
      {status && (
        <p role="status" className="p-6 text-center text-sm text-fg-muted">
          {status}
        </p>
      )}
      <Menu
        aria-label="Books"
        items={items}
        className="max-h-72 overflow-y-auto p-1 outline-none"
        onClose={onClose}
      >
        {(hit: SearchItem) => (
          <MenuItem
            id={hit.id}
            href={`/b/${encodeURIComponent(hit.id)}`}
            textValue={hit.title}
            className="cursor-default rounded-md px-2 py-1.5 outline-none data-focused:bg-muted"
          >
            <Text slot="label">
              <span className="block text-sm font-medium">{hit.title}</span>{" "}
              <span className="block text-xs text-fg-muted">{hit.authors.join(", ")}</span>
              {hit.snippet && (
                <HighlightedSnippet text={hit.snippet} className="block text-xs text-fg-muted" />
              )}
            </Text>
          </MenuItem>
        )}
      </Menu>
    </>
  );
}

type HighlightedSnippetProps = {
  text: string;
  className?: string;
};

/**
 * Split a `ts_headline` snippet on the STX/ETX markers Postgres
 * emits and render the highlighted runs as `<mark>`. Plain text-node
 * rendering (no `dangerouslySetInnerHTML`) — safe against
 * HTML-bearing titles or descriptions.
 */
export function HighlightedSnippet({
  text,
  className,
}: Readonly<HighlightedSnippetProps>): ReactElement {
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
