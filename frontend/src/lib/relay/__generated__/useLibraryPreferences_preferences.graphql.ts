/**
 * @generated SignedSource<<9f5c2e5042698bd2c618d37908ce006c>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
export type Direction = "ASC" | "DESC" | "%future added value";
export type LibraryColumn =
  | "ADDED"
  | "AUTHORS"
  | "ISBN_13"
  | "PAGES"
  | "RATING"
  | "SERIES"
  | "STATUS"
  | "SUBTITLE"
  | "%future added value";
export type LibraryDensity = "COMFORTABLE" | "COMPACT" | "%future added value";
export type LibraryView = "GRID" | "TABLE" | "%future added value";
export type SortField = "AUTHOR" | "CREATED_AT" | "PAGES" | "TITLE" | "%future added value";
import { FragmentRefs } from "relay-runtime";
export type useLibraryPreferences_preferences$data = {
  readonly defaults: {
    readonly density: LibraryDensity;
    readonly hiddenColumns: ReadonlyArray<LibraryColumn>;
    readonly sort: ReadonlyArray<{
      readonly column: SortField;
      readonly direction: Direction;
    }>;
    readonly view: LibraryView;
  };
  readonly density: LibraryDensity | null | undefined;
  readonly hiddenColumns: ReadonlyArray<LibraryColumn> | null | undefined;
  readonly sort:
    | ReadonlyArray<{
        readonly column: SortField;
        readonly direction: Direction;
      }>
    | null
    | undefined;
  readonly view: LibraryView | null | undefined;
  readonly " $fragmentType": "useLibraryPreferences_preferences";
};
export type useLibraryPreferences_preferences$key = {
  readonly " $data"?: useLibraryPreferences_preferences$data;
  readonly " $fragmentSpreads": FragmentRefs<"useLibraryPreferences_preferences">;
};

const node: ReaderFragment = (function () {
  var v0 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      concreteType: "LibrarySortLevel",
      kind: "LinkedField",
      name: "sort",
      plural: true,
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "column",
          storageKey: null,
        },
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "direction",
          storageKey: null,
        },
      ],
      storageKey: null,
    };
  return {
    argumentDefinitions: [],
    kind: "Fragment",
    metadata: {
      throwOnFieldError: true,
    },
    name: "useLibraryPreferences_preferences",
    selections: [
      v0 /*:: as any*/,
      v1 /*:: as any*/,
      v2 /*:: as any*/,
      v3 /*:: as any*/,
      {
        alias: null,
        args: null,
        concreteType: "LibraryPreferenceDefaults",
        kind: "LinkedField",
        name: "defaults",
        plural: false,
        selections: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/, v3 /*:: as any*/],
        storageKey: null,
      },
    ],
    type: "LibraryPreferences",
    abstractKey: null,
  };
})();

(node as any).hash = "813ffb81d218cf7d99b5e24589154bed";

export default node;
