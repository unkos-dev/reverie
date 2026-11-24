/**
 * Global library search using server-ranked results and safe snippet highlighting.
 * React Aria owns modal focus and dismissal.
 */
import {
  graphql,
  useFragment,
  usePreloadedQuery,
  useQueryLoader,
  type PreloadedQuery,
} from "react-relay";
import type { CommandPaletteQuery as SearchQuery } from "@/lib/relay/__generated__/CommandPaletteQuery.graphql";
import { Suspense, useState, type ReactElement, type ReactNode } from "react";
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

import type { CommandPalette_results$key } from "@/lib/relay/__generated__/CommandPalette_results.graphql";
import type {
  CommandPalette_hit$data,
  CommandPalette_hit$key,
} from "@/lib/relay/__generated__/CommandPalette_hit.graphql";

const commandPaletteQuery = graphql`
  query CommandPaletteQuery($q: String!) @throwOnFieldError {
    ...CommandPalette_results @arguments(q: $q)
  }
`;

const resultsFragment = graphql`
  fragment CommandPalette_results on Query
  @argumentDefinitions(q: { type: "String!" })
  @throwOnFieldError {
    search(q: $q) {
      manifestation {
        id
        work {
          id
        }
      }
      ...CommandPalette_hit
    }
  }
`;

const hitFragment = graphql`
  fragment CommandPalette_hit on SearchHit @throwOnFieldError {
    snippet {
      text
      highlighted
    }
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
  const query = usePreloadedQuery<SearchQuery>(commandPaletteQuery, reference);
  const { search } = useFragment<CommandPalette_results$key>(resultsFragment, query);
  const hits = search.filter(({ manifestation }) => manifestation?.work != null);
  return (
    <SearchResults status={hits.length === 0 ? "No results." : null} onClose={onClose}>
      {hits.map((hit) => (
        <SearchResult key={hit.manifestation?.id} hit={hit} />
      ))}
    </SearchResults>
  );
}

function SearchResults({
  status,
  children,
  onClose,
}: Readonly<{ status: string | null; children?: ReactNode; onClose: () => void }>): ReactElement {
  return (
    <>
      {status && (
        <p role="status" className="p-6 text-center text-sm text-fg-muted">
          {status}
        </p>
      )}
      <Menu
        aria-label="Books"
        className="max-h-72 overflow-y-auto p-1 outline-none"
        onClose={onClose}
      >
        {children}
      </Menu>
    </>
  );
}

function SearchResult({ hit }: Readonly<{ hit: CommandPalette_hit$key }>): ReactElement | null {
  const { manifestation, snippet } = useFragment(hitFragment, hit);
  if (manifestation?.work == null) return null;
  const { id, work } = manifestation;
  return (
    <MenuItem
      id={id}
      href={`/b/${encodeURIComponent(id)}`}
      textValue={work.title}
      className="cursor-default rounded-md px-2 py-1.5 outline-none data-focused:bg-muted"
    >
      <Text slot="label">
        <span className="block text-sm font-medium">{work.title}</span>{" "}
        <span className="block text-xs text-fg-muted">
          {work.authors.edges.map(({ node }) => node.name).join(", ")}
        </span>
        {snippet != null && snippet.length > 0 && (
          <HighlightedSnippet parts={snippet} className="block text-xs text-fg-muted" />
        )}
      </Text>
    </MenuItem>
  );
}

type HighlightedSnippetProps = {
  parts: NonNullable<CommandPalette_hit$data["snippet"]>;
  className?: string;
};

/** Render the server's highlight runs as text nodes and marks. */
export function HighlightedSnippet({
  parts,
  className,
}: Readonly<HighlightedSnippetProps>): ReactElement {
  const runs: ReactElement[] = [];
  let start = 0;
  for (const part of parts) {
    const key = String(start);
    start += part.text.length + 1;
    runs.push(
      part.highlighted ? (
        <mark key={key} className="bg-accent-soft text-fg rounded-sm px-0.5">
          {part.text}
        </mark>
      ) : (
        <span key={key}>{part.text}</span>
      ),
    );
  }
  return <span className={className}>{runs}</span>;
}
