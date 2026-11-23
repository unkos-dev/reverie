/**
 * @generated SignedSource<<ec2ae6e3c4709b3b0af5bb304349ec3e>>
 * @relayHash 6434ba7f3e282eec67c4603ea3b50057
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 3852637eb2a35a5342282e2ee916d1f3e8bb8c3c364439a78e5ecfc32f8b8aa5

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
export type mutationsUpdateLibraryPreferencesMutation = {
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
      id: "3852637eb2a35a5342282e2ee916d1f3e8bb8c3c364439a78e5ecfc32f8b8aa5",
      metadata: {},
      name: "mutationsUpdateLibraryPreferencesMutation",
      operationKind: "mutation",
      text: null,
    },
  };
})();

(node as any).hash = "f87c73d223d7ec5258011ce70489ad46";

export default node;
