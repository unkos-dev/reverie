/**
 * The library's per-user display preferences: hidden columns, row density, view choice and sort.
 *
 * The server owns them, so they follow the reader between devices. Each group resolves as the
 * reader's override, else the installation default that arrives with it. Each gesture sends one
 * mutation with a single-field optimistic store update, which Relay rolls back if the write fails.
 *
 * No value here is ever written to the browser URL. A Library visit's own view override wins over
 * the stored view while the visit holds one. Sort has no URL form at all: it is a one-layer
 * per-user preference, resolved here and sent explicitly on the list request (see
 * `docs/adr/0047-library-sort-is-a-per-user-preference-never-url-state.md`).
 */
import { graphql, useFragment, useMutation } from "react-relay";
import type { RecordProxy, RecordSourceSelectorProxy } from "relay-runtime";
import { mutationsUpdateLibraryPreferencesMutation } from "@/lib/relay/mutations";
import type {
  useLibraryPreferences_preferences$data as PreferencesData,
  useLibraryPreferences_preferences$key,
} from "@/lib/relay/__generated__/useLibraryPreferences_preferences.graphql";
import type {
  mutationsUpdateLibraryPreferencesMutation as PreferencesMutation,
  UpdateLibraryPreferencesInput,
} from "@/lib/relay/__generated__/mutationsUpdateLibraryPreferencesMutation.graphql";
import { useMemo } from "react";

import {
  parseSortParam,
  serializeSortParam,
  type Density,
  type LibraryView,
  type SortLevelParam,
} from "@/api";
import { HIDEABLE_COLUMNS } from "./table-columns";

const preferencesFragment = graphql`
  fragment useLibraryPreferences_preferences on LibraryPreferences @throwOnFieldError {
    hiddenColumns
    density
    view
    sort {
      column
      direction
    }
    defaults {
      hiddenColumns
      density
      view
      sort {
        column
        direction
      }
    }
  }
`;

type Column = NonNullable<PreferencesData["hiddenColumns"]>[number];
type SortColumn = PreferencesData["defaults"]["sort"][number]["column"];
type PreferencesRecord = RecordProxy<Record<never, never>>;

const COLUMNS = new Map<string, Column>([
  ["subtitle", "SUBTITLE"],
  ["authors", "AUTHORS"],
  ["series", "SERIES"],
  ["isbn_13", "ISBN_13"],
  ["pages", "PAGES"],
  ["status", "STATUS"],
  ["rating", "RATING"],
  ["added", "ADDED"],
]);

const SORT_COLUMNS = {
  title: "TITLE",
  author: "AUTHOR",
  created_at: "CREATED_AT",
  pages: "PAGES",
} as const satisfies Record<SortLevelParam["field"], SortColumn>;

function columnKeys(columns: readonly Column[]): string[] {
  return HIDEABLE_COLUMNS.flatMap(({ key }) => {
    const column = COLUMNS.get(key);
    return column !== undefined && columns.includes(column) ? [key] : [];
  });
}

function sortLevelParams(levels: PreferencesData["defaults"]["sort"]): SortLevelParam[] {
  return levels.flatMap((level) => {
    const field = (Object.keys(SORT_COLUMNS) as SortLevelParam["field"][]).find(
      (key) => SORT_COLUMNS[key] === level.column,
    );
    return field === undefined ? [] : [{ field, desc: level.direction === "DESC" }];
  });
}

export type LibraryPreferences = {
  /** Effective row density for the table view. */
  density: Density;
  /** Effective hidden-column keys. */
  hiddenColumns: ReadonlySet<string>;
  /** Effective view choice, which a Library visit's view override still overrides. */
  view: LibraryView;
  /**
   * The reader's sort override in the `?sort=` wire grammar, or `""` when they inherit the
   * installation order. An inheriting reader sends no `sort` at all.
   */
  sortOverride: string;
  /** The effective sort stack every sort surface renders and edits. */
  sortLevels: readonly SortLevelParam[];
  /** The installation default sort stack. */
  defaultSortLevels: readonly SortLevelParam[];
  /** True when the effective sort is the inherited installation stack. */
  sortInherited: boolean;
  /** Whether a columns override exists to drop, even one equal to the default. */
  canResetColumns: boolean;
  setDensity: (next: Density) => void;
  setColumnHidden: (key: string, hidden: boolean) => void;
  /** Drop the columns override so the group inherits the default again. */
  resetColumns: () => void;
  setView: (next: LibraryView) => void;
  /**
   * The one intent handler for every sort gesture. A non-empty stack is written as the reader's
   * override; an empty stack drops it, and the display becomes the installation stack.
   */
  setSortLevels: (levels: readonly SortLevelParam[]) => void;
};

export function useLibraryPreferences(
  viewerId: string,
  key: useLibraryPreferences_preferences$key,
): LibraryPreferences {
  const data = useFragment<useLibraryPreferences_preferences$key>(preferencesFragment, key);
  const [commit] = useMutation<PreferencesMutation>(mutationsUpdateLibraryPreferencesMutation);

  function write(
    input: UpdateLibraryPreferencesInput,
    apply: (record: PreferencesRecord, store: RecordSourceSelectorProxy) => void,
  ): void {
    commit({
      variables: { input },
      optimisticUpdater: (store) => {
        const record: PreferencesRecord | null | undefined = store
          .get(viewerId)
          ?.getLinkedRecord("libraryPreferences");
        if (record != null) apply(record, store);
      },
      onCompleted: ({ updateLibraryPreferences }) => {
        if (updateLibraryPreferences.__typename === "LibraryPreferencesInvalid")
          console.error(
            "[useLibraryPreferences] a preference was refused",
            updateLibraryPreferences.message,
          );
      },
      onError: (error) => {
        console.error("[useLibraryPreferences] failed to persist a preference", error);
      },
    });
  }

  const density: Density =
    (data.density ?? data.defaults.density) === "COMPACT" ? "compact" : "comfortable";
  const view: LibraryView = (data.view ?? data.defaults.view) === "TABLE" ? "table" : "grid";
  const storedColumns = data.hiddenColumns ?? data.defaults.hiddenColumns;
  const hiddenKeys = columnKeys(storedColumns).join(",");
  const hiddenColumns = useMemo(
    () => new Set(hiddenKeys === "" ? [] : hiddenKeys.split(",")),
    [hiddenKeys],
  );
  const sortOverride = serializeSortParam(sortLevelParams(data.sort ?? []));
  const defaultSort = serializeSortParam(sortLevelParams(data.defaults.sort));
  const effectiveSort = sortOverride === "" ? defaultSort : sortOverride;
  const sortLevels = useMemo(() => parseSortParam(effectiveSort), [effectiveSort]);
  const defaultSortLevels = useMemo(() => parseSortParam(defaultSort), [defaultSort]);

  return {
    density,
    hiddenColumns,
    view,
    sortOverride,
    sortLevels,
    defaultSortLevels,
    sortInherited: sortOverride === "",
    canResetColumns: data.hiddenColumns != null,
    setDensity: (next) => {
      const value = next === "compact" ? "COMPACT" : "COMFORTABLE";
      write({ density: value }, (record) => {
        record.setValue(value, "density");
      });
    },
    setColumnHidden: (columnKey, hidden) => {
      const column = COLUMNS.get(columnKey);
      if (column === undefined) return;
      write({ columns: [{ column, hidden }] }, (record) => {
        const stored = record.getValue("hiddenColumns");
        const current: readonly string[] = Array.isArray(stored)
          ? stored.filter((item): item is string => typeof item === "string")
          : data.defaults.hiddenColumns;
        const others = current.filter((item) => item !== column);
        record.setValue(hidden ? [...others, column] : others, "hiddenColumns");
      });
    },
    resetColumns: () => {
      write({ resetHiddenColumns: true }, (record) => {
        record.setValue(null, "hiddenColumns");
      });
    },
    setView: (next) => {
      const value = next === "table" ? "TABLE" : "GRID";
      write({ view: value }, (record) => {
        record.setValue(value, "view");
      });
    },
    setSortLevels: (levels) => {
      const sort =
        levels.length === 0
          ? null
          : levels.map((level) => ({
              column: SORT_COLUMNS[level.field],
              direction: level.desc ? ("DESC" as const) : ("ASC" as const),
            }));
      write({ sort }, (record, store) => {
        if (sort === null) {
          record.setValue(null, "sort");
          return;
        }
        const entries = sort.map((level, index) => {
          const id = `${record.getDataID()}:sort:${String(index)}`;
          const entry = store.get(id) ?? store.create(id, "LibrarySortLevel");
          entry.setValue(level.column, "column");
          entry.setValue(level.direction, "direction");
          return entry;
        });
        record.setLinkedRecords(entries, "sort");
      });
    },
  };
}
