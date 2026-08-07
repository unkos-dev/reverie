/**
 * Debounced quick-search input over the library's `q` filter.
 *
 * Owns the `q` key outright (see `lib/hooks/use-library-filters`), so it
 * takes no props and coordinates with nothing: a sibling filter edit
 * writes other keys and cannot disturb what is being typed here.
 */
import { Search } from "lucide-react";
import { useState, type ReactElement } from "react";

import { Input } from "@/components/ui/input";
import { projectQuery, useQuickSearchFilter } from "@/lib/hooks/use-library-filters";

export function QuickSearch(): ReactElement {
  const { query, setQuery } = useQuickSearchFilter();
  // Below the minimum length the box deliberately holds text that is not a
  // filter, so the input cannot just render `query`.
  const [text, setText] = useState(query ?? "");
  const [syncedQuery, setSyncedQuery] = useState(query);
  const projected = projectQuery(text);
  // Resync only when the committed value moved somewhere this box did not
  // put it: a clear affordance, a chip removal, or navigation. Comparing it
  // against what the current text projects to tells that apart from our own
  // write landing, without asking who wrote it. Render-phase state
  // adjustment, the compiler-accepted alternative to a sync effect.
  if (query !== syncedQuery) {
    setSyncedQuery(query);
    if (query !== projected) setText(query ?? "");
  }

  return (
    <div className="relative min-w-0 flex-1">
      <Search
        aria-hidden="true"
        className="text-fg-muted pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2"
      />
      <Input
        type="search"
        aria-label="Search your library"
        placeholder="Search your library"
        className="h-9 pl-9"
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setQuery(projectQuery(event.target.value));
        }}
      />
    </div>
  );
}
