/**
 * @generated SignedSource<<47fdad5206889e410de2f2f2d584d387>>
 * @relayHash 82ea05d6b3ed79952e0bb72f5141a4b3
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e1175989e8c4021b1c1c2109c1ffbd75bc6ea4aab77aaaaabf3a0aaa35ff9924

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
export type useLibraryPreferencesMutation$variables = {
  input: UpdateLibraryPreferencesInput;
};
export type useLibraryPreferencesMutation$data = {
  readonly updateLibraryPreferences:
    | {
        readonly __typename: "LibraryPreferencesInvalid";
        readonly message: string;
      }
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
        // This will never be '%other', but we need some
        // value in case none of the concrete values match.
        readonly __typename: "%other";
      };
};
export type useLibraryPreferencesMutation = {
  response: useLibraryPreferencesMutation$data;
  variables: useLibraryPreferencesMutation$variables;
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
      type: "LibraryPreferencesInvalid",
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
      name: "useLibraryPreferencesMutation",
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
      name: "useLibraryPreferencesMutation",
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
                        v2 /*:: as any*/,
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
      id: "e1175989e8c4021b1c1c2109c1ffbd75bc6ea4aab77aaaaabf3a0aaa35ff9924",
      metadata: {},
      name: "useLibraryPreferencesMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "9ec1e3cf190793667e98768d5a114d5b";

export default node;
