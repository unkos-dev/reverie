/**
 * Global library search using server-ranked results and safe snippet highlighting.
 * React Aria owns modal focus and dismissal; application shortcuts respect the focused field.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState, type ReactElement } from "react";
import { useNavigate } from "react-router";
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

import { searchLibrary, type SearchHit } from "@/api";
import { setCommandPaletteOpener } from "@/lib/command-palette";
import { queryKeys } from "@/lib/query/keys";

const SNIPPET_HL_START = "\u0002";
const SNIPPET_HL_END = "\u0003";

/** Searches from two characters and navigates to the selected book without changing library filters. */
export function CommandPalette(): ReactElement {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const trimmed = query.trim();
  const navigate = useNavigate();
  const { data, isFetching, isError, error } = useQuery({
    queryKey: queryKeys.search(trimmed),
    queryFn: ({ signal }) => searchLibrary(trimmed, signal),
    enabled: open && trimmed.length >= 2,
    staleTime: 30_000,
  });
  const items = trimmed.length >= 2 ? (data?.items ?? []) : [];
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent): void {
      const cmdK = (event.key === "k" || event.key === "K") && (event.metaKey || event.ctrlKey);
      const target = document.activeElement;
      const inField =
        target instanceof HTMLElement &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      const slash =
        event.key === "/" && !event.metaKey && !event.ctrlKey && !event.altKey && !inField;
      if (!cmdK && !slash) return;
      event.preventDefault();
      setOpen(cmdK ? !open : true);
    }
    window.addEventListener("keydown", onKeyDown);
    setCommandPaletteOpener(() => {
      setOpen(true);
    });
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      setCommandPaletteOpener(() => {});
    };
  }, [open]);
  useEffect(() => {
    if (isError) console.error("[CommandPalette] search query failed", error);
  }, [isError, error]);

  function changeOpen(next: boolean): void {
    setOpen(next);
    if (!next) setQuery("");
  }
  let status: string | null = null;
  if (trimmed.length < 2) status = "Type to search your library.";
  else if (isError) status = "Search failed. Try again in a moment.";
  else if (isFetching && items.length === 0) status = "Searching…";
  else if (items.length === 0) status = "No results.";

  return (
    <ModalOverlay
      isOpen={open}
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
          <Autocomplete inputValue={query} onInputChange={setQuery}>
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
            {status && (
              <p role="status" className="p-6 text-center text-sm text-fg-muted">
                {status}
              </p>
            )}
            <Menu
              aria-label="Books"
              items={items}
              className="max-h-72 overflow-y-auto p-1 outline-none"
              onAction={(key) => {
                changeOpen(false);
                void navigate(`/b/${encodeURIComponent(String(key))}`);
              }}
            >
              {(hit: SearchHit) => (
                <MenuItem
                  id={hit.id}
                  textValue={hit.title}
                  className="cursor-default rounded-md px-2 py-1.5 outline-none data-focused:bg-muted"
                >
                  <Text slot="label">
                    <span className="block text-sm font-medium">{hit.title}</span>{" "}
                    <span className="block text-xs text-fg-muted">{hit.authors.join(", ")}</span>
                    {hit.snippet && (
                      <HighlightedSnippet
                        text={hit.snippet}
                        className="block text-xs text-fg-muted"
                      />
                    )}
                  </Text>
                </MenuItem>
              )}
            </Menu>
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
