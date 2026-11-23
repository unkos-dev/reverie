/**
 * @generated SignedSource<<000c85aaf5808edc4849ee1145a88391>>
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

import { ReaderFragment } from "relay-runtime";
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
import { FragmentRefs } from "relay-runtime";
export type useLibraryPreferences_updatable$data = {
  density: LibraryDensity | null | undefined;
  hiddenColumns: ReadonlyArray<LibraryColumn> | null | undefined;
  view: LibraryView | null | undefined;
  readonly " $fragmentType": "useLibraryPreferences_updatable";
};
export type useLibraryPreferences_updatable$key = {
  readonly " $data"?: useLibraryPreferences_updatable$data;
  readonly $updatableFragmentSpreads: FragmentRefs<"useLibraryPreferences_updatable">;
};

const node: ReaderFragment = {
  argumentDefinitions: [],
  kind: "Fragment",
  metadata: null,
  name: "useLibraryPreferences_updatable",
  selections: [
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
  ],
  type: "LibraryPreferences",
  abstractKey: null,
};

(node as any).hash = "0e585c111d674c7b23a4c6cecc8b6b1c";

export default node;
