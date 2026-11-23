/**
 * @generated SignedSource<<9876a385573b68007de9c0dca89af00e>>
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
  readonly $updatableFragmentSpreads: FragmentRefs<"useLibraryPreferences_updatable">;
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
      {
        args: null,
        kind: "FragmentSpread",
        name: "useLibraryPreferences_updatable",
      },
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

(node as any).hash = "ec86cb1c4a15b59daa51af8d2f7a4cf2";

export default node;
