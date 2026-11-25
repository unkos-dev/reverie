/**
 * @generated SignedSource<<e7353171de3b9e777780dc92d4c2ce3b>>
 * @relayHash ed660a9ad50010c6105f4e4408ff1299
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 1303bc50c3bb8e8d061c0a6ed3340cc08ef48accb10226f0c4ff473c1eccabd2

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type Direction = "ASC" | "DESC" | "%future added value";
export type SortField =
  | "AUTHOR"
  | "CREATED_AT"
  | "ISBN_13"
  | "PAGES"
  | "RATING"
  | "SERIES"
  | "STATUS"
  | "SUBTITLE"
  | "TITLE"
  | "%future added value";
export type libraryBootstrapQuery$variables = {
  authorIds: ReadonlyArray<string>;
  seriesIds: ReadonlyArray<string>;
  shelfIds: ReadonlyArray<string>;
};
export type libraryBootstrapQuery$data = {
  readonly authorLabels: ReadonlyArray<{
    readonly id: string;
  }>;
  readonly seriesLabels: ReadonlyArray<{
    readonly id: string;
  }>;
  readonly shelfLabels: ReadonlyArray<{
    readonly id: string;
  }>;
  readonly viewer: {
    readonly id: string;
    readonly libraryPreferences: {
      readonly defaults: {
        readonly sort: ReadonlyArray<{
          readonly column: SortField;
          readonly direction: Direction;
        }>;
      };
      readonly sort:
        | ReadonlyArray<{
            readonly column: SortField;
            readonly direction: Direction;
          }>
        | null
        | undefined;
      readonly " $fragmentSpreads": FragmentRefs<"useLibraryPreferences_preferences">;
    };
  };
};
export type libraryBootstrapQuery = {
  response: libraryBootstrapQuery$data;
  variables: libraryBootstrapQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "authorIds",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "seriesIds",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "shelfIds",
      },
    ],
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v2 = {
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
    },
    v3 = [v1 /*:: as any*/],
    v4 = {
      alias: null,
      args: [
        {
          kind: "Variable",
          name: "ids",
          variableName: "authorIds",
        },
      ],
      concreteType: "Author",
      kind: "LinkedField",
      name: "authorLabels",
      plural: true,
      selections: v3 /*:: as any*/,
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: [
        {
          kind: "Variable",
          name: "ids",
          variableName: "seriesIds",
        },
      ],
      concreteType: "Series",
      kind: "LinkedField",
      name: "seriesLabels",
      plural: true,
      selections: v3 /*:: as any*/,
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: [
        {
          kind: "Variable",
          name: "ids",
          variableName: "shelfIds",
        },
      ],
      concreteType: "Shelf",
      kind: "LinkedField",
      name: "shelfLabels",
      plural: true,
      selections: v3 /*:: as any*/,
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "libraryBootstrapQuery",
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "Viewer",
          kind: "LinkedField",
          name: "viewer",
          plural: false,
          selections: [
            v1 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferences",
              kind: "LinkedField",
              name: "libraryPreferences",
              plural: false,
              selections: [
                {
                  args: null,
                  kind: "FragmentSpread",
                  name: "useLibraryPreferences_preferences",
                },
                v2 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "LibraryPreferenceDefaults",
                  kind: "LinkedField",
                  name: "defaults",
                  plural: false,
                  selections: [v2 /*:: as any*/],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        v4 /*:: as any*/,
        v5 /*:: as any*/,
        v6 /*:: as any*/,
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "libraryBootstrapQuery",
      selections: [
        {
          alias: null,
          args: null,
          concreteType: "Viewer",
          kind: "LinkedField",
          name: "viewer",
          plural: false,
          selections: [
            v1 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "LibraryPreferences",
              kind: "LinkedField",
              name: "libraryPreferences",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "__typename",
                  storageKey: null,
                },
                v7 /*:: as any*/,
                v8 /*:: as any*/,
                v9 /*:: as any*/,
                v2 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "LibraryPreferenceDefaults",
                  kind: "LinkedField",
                  name: "defaults",
                  plural: false,
                  selections: [
                    v7 /*:: as any*/,
                    v8 /*:: as any*/,
                    v9 /*:: as any*/,
                    v2 /*:: as any*/,
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        v4 /*:: as any*/,
        v5 /*:: as any*/,
        v6 /*:: as any*/,
      ],
    },
    params: {
      id: "1303bc50c3bb8e8d061c0a6ed3340cc08ef48accb10226f0c4ff473c1eccabd2",
      metadata: {},
      name: "libraryBootstrapQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "62718553d8f5b6aae07b8cf65b5b06a0";

export default node;
