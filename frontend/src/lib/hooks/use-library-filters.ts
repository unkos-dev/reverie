import { useState } from "react";

import {
  emptyFilterState,
  LibraryVisitSchema,
  type FilterState,
  type LibraryVisit,
  type LibraryView,
  type LibraryPosition,
} from "@/routes/library-params";

const VISIT_KEY = "reverieLibrary";

function entryData(): Record<string, unknown> {
  const state: unknown = navigation.currentEntry?.getState();
  return typeof state === "object" && state !== null
    ? Object.fromEntries(Object.entries(state))
    : {};
}

/** Decode the current Navigation entry's saved visit, or an empty visit when it is unusable. */
export function restoreVisit(): LibraryVisit {
  const parsed = LibraryVisitSchema.safeParse(entryData()[VISIT_KEY]);
  return parsed.success ? parsed.data : { filters: emptyFilterState() };
}

/** The one writer of a Library visit's intent on its own Navigation entry. */
export class LibraryIntent {
  private entry: string | undefined;
  private visit: LibraryVisit;

  constructor(entry: string | undefined, visit: LibraryVisit) {
    this.entry = entry;
    this.visit = visit;
  }

  get entryId(): string | undefined {
    return this.entry;
  }

  bind(entry: string): void {
    this.entry = entry;
  }

  current(): LibraryVisit {
    return this.visit;
  }

  restore(visit: LibraryVisit): void {
    this.visit = visit;
  }

  update(patch: (visit: LibraryVisit) => LibraryVisit): LibraryVisit | undefined {
    if (this.entry === undefined || navigation.currentEntry?.id !== this.entry) return undefined;
    const next = patch(this.visit);
    navigation.updateCurrentEntry({ state: { ...entryData(), [VISIT_KEY]: next } });
    this.visit = next;
    return next;
  }
}

export type LibraryFilters = {
  filters: FilterState;
  view: LibraryView | undefined;
  position: LibraryPosition | undefined;
  capturePosition: (position: LibraryPosition) => void;
  commitAll: (patch: (current: FilterState) => FilterState) => void;
  clearAll: () => void;
  setView: (view: LibraryView) => void;
};

/**
 * Render the visit a route load restored for `intent`, and dispatch edits through it. `follow`
 * receives each filter edit's result before the edit is written.
 */
export function useLibraryFilters(
  intent: LibraryIntent,
  restored: LibraryVisit,
  follow: (next: FilterState, write: () => void) => void,
): LibraryFilters {
  const [snapshot, setSnapshot] = useState({ restored, visit: restored });
  let rendered = snapshot;
  if (snapshot.restored !== restored) {
    rendered = { restored, visit: restored };
    setSnapshot(rendered);
  }

  function update(patch: (visit: LibraryVisit) => LibraryVisit): void {
    const next = intent.update(patch);
    if (next !== undefined) setSnapshot({ restored, visit: next });
  }

  return {
    filters: rendered.visit.filters,
    view: rendered.visit.view,
    position: rendered.visit.position,
    capturePosition: (position) => {
      update((visit) => ({ ...visit, position }));
    },
    commitAll: (patch) => {
      follow(patch(intent.current().filters), () => {
        update((visit) => ({ ...visit, filters: patch(visit.filters), position: undefined }));
      });
    },
    clearAll: () => {
      follow(emptyFilterState(), () => {
        update((visit) => ({ ...visit, filters: emptyFilterState(), position: undefined }));
      });
    },
    setView: (view) => {
      update((visit) => ({ ...visit, view, position: undefined }));
    },
  };
}
