import { useEffect, useRef, useState } from "react";
import { useLocation } from "react-router";

import {
  emptyFilterState,
  LibraryVisitSchema,
  type FilterState,
  type LibraryVisit,
  type LibraryView,
} from "@/routes/library-params";

const VISIT_KEY = "reverieLibrary";

function entryData(): Record<string, unknown> {
  const state: unknown = navigation.currentEntry?.getState();
  return typeof state === "object" && state !== null
    ? Object.fromEntries(Object.entries(state))
    : {};
}

function restoreVisit(): LibraryVisit {
  const parsed = LibraryVisitSchema.safeParse(entryData()[VISIT_KEY]);
  return parsed.success ? parsed.data : { filters: emptyFilterState() };
}

type VisitSnapshot = { entryKey: string | undefined; visit: LibraryVisit };

function restoreSnapshot(): VisitSnapshot {
  return { entryKey: navigation.currentEntry?.key, visit: restoreVisit() };
}

export type LibraryFilters = {
  filters: FilterState;
  view: LibraryView | undefined;
  commitAll: (patch: (current: FilterState) => FilterState) => void;
  clearAll: () => void;
  setView: (view: LibraryView) => void;
};

export function useLibraryFilters(): LibraryFilters {
  useLocation();
  const [snapshot, setSnapshot] = useState(restoreSnapshot);
  const current = useRef(snapshot);
  let rendered = snapshot;
  if (snapshot.entryKey !== navigation.currentEntry?.key) {
    rendered = restoreSnapshot();
    setSnapshot(rendered);
  }

  useEffect(() => {
    function restore(event: NavigationCurrentEntryChangeEvent): void {
      if (event.navigationType === null) return;
      const next = restoreSnapshot();
      current.current = next;
      setSnapshot(next);
    }
    navigation.addEventListener("currententrychange", restore);
    return () => {
      navigation.removeEventListener("currententrychange", restore);
    };
  }, []);

  function update(patch: (visit: LibraryVisit) => LibraryVisit): void {
    const prior =
      current.current.entryKey === navigation.currentEntry?.key
        ? current.current
        : restoreSnapshot();
    const next = { entryKey: prior.entryKey, visit: patch(prior.visit) };
    navigation.updateCurrentEntry({ state: { ...entryData(), [VISIT_KEY]: next.visit } });
    current.current = next;
    setSnapshot(next);
  }

  return {
    filters: rendered.visit.filters,
    view: rendered.visit.view,
    commitAll: (patch) => {
      update((visit) => ({ ...visit, filters: patch(visit.filters) }));
    },
    clearAll: () => {
      update((visit) => ({ ...visit, filters: emptyFilterState() }));
    },
    setView: (view) => {
      update((visit) => ({ ...visit, view }));
    },
  };
}
