/**
 * @generated SignedSource<<be7f68a39c04ce44144cd1927577fcf4>>
 * @relayHash 8a17e47111437fe7e6d357479a46c827
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c3992fcab5ca8e7f389f8092a5b56ca06ee325421d585c937a3a01664b118b06

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
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
export type UpdateLibraryPreferencesInput = {
  columns?: ReadonlyArray<LibraryColumnVisibility> | null | undefined;
  density?: LibraryDensity | null | undefined;
  resetHiddenColumns?: boolean | null | undefined;
  sort?: ReadonlyArray<SortLevel> | null | undefined;
  view?: LibraryView | null | undefined;
};
export type SortLevel = {
  column: SortField;
  direction: Direction;
};
export type LibraryColumnVisibility = {
  column: LibraryColumn;
  hidden: boolean;
};
export type mutationsUpdateLibraryPreferencesMutation$variables = {
  input: UpdateLibraryPreferencesInput;
};
export type mutationsUpdateLibraryPreferencesMutation$data = {
  readonly updateLibraryPreferences:
    | {
        readonly __typename: "LibraryPreferencesUpdated";
        readonly viewer: {
          readonly id: string;
          readonly libraryPreferences: {
            readonly " $fragmentSpreads": FragmentRefs<"useLibraryPreferences_preferences">;
          };
        };
      }
    | {
        readonly __typename: "MetadataInvalid";
        readonly message: string;
      }
    | {
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type mutationsUpdateLibraryPreferencesMutation$rawResponse = {
  readonly updateLibraryPreferences:
    | {
        readonly __typename: "LibraryPreferencesUpdated";
        readonly viewer: {
          readonly id: string;
          readonly libraryPreferences: {
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
          };
        };
      }
    | {
        readonly __typename: "MetadataInvalid";
        readonly message: string;
      }
    | {
        readonly __typename: string;
      };
};
export type mutationsUpdateLibraryPreferencesMutation = {
  rawResponse: mutationsUpdateLibraryPreferencesMutation$rawResponse;
  response: mutationsUpdateLibraryPreferencesMutation$data;
  variables: mutationsUpdateLibraryPreferencesMutation$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "input",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "input",
        variableName: "input",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v4 = {
      kind: "InlineFragment",
      selections: [
        {
          alias: null,
          args: null,
          kind: "ScalarField",
          name: "message",
          storageKey: null,
        },
      ],
      type: "MetadataInvalid",
      abstractKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v8 = {
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
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "mutationsUpdateLibraryPreferencesMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "updateLibraryPreferences",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "Viewer",
                  kind: "LinkedField",
                  name: "viewer",
                  plural: false,
                  selections: [
                    v3 /*:: as any*/,
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
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "LibraryPreferencesUpdated",
              abstractKey: null,
            },
            v4 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
      type: "Mutation",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "mutationsUpdateLibraryPreferencesMutation",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: null,
          kind: "LinkedField",
          name: "updateLibraryPreferences",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "Viewer",
                  kind: "LinkedField",
                  name: "viewer",
                  plural: false,
                  selections: [
                    v3 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      concreteType: "LibraryPreferences",
                      kind: "LinkedField",
                      name: "libraryPreferences",
                      plural: false,
                      selections: [
                        v5 /*:: as any*/,
                        v6 /*:: as any*/,
                        v7 /*:: as any*/,
                        v8 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          concreteType: "LibraryPreferenceDefaults",
                          kind: "LinkedField",
                          name: "defaults",
                          plural: false,
                          selections: [
                            v5 /*:: as any*/,
                            v6 /*:: as any*/,
                            v7 /*:: as any*/,
                            v8 /*:: as any*/,
                          ],
                          storageKey: null,
                        },
                      ],
                      storageKey: null,
                    },
                  ],
                  storageKey: null,
                },
              ],
              type: "LibraryPreferencesUpdated",
              abstractKey: null,
            },
            v4 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c3992fcab5ca8e7f389f8092a5b56ca06ee325421d585c937a3a01664b118b06",
      metadata: {},
      name: "mutationsUpdateLibraryPreferencesMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "8624403d0bd9ab1f472df20042afe255";

export default node;
