/**
 * @generated SignedSource<<cacbd526bcd0226916e47f2014c548b8>>
 * @relayHash 906ef492db211d7a18929b77826b3ab9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID e1ab7f2fa99255692c48547aaefd59426c0f3f200bb23c942c2f32161ed37b6e

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
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
    v2 = [v1 /*:: as any*/],
    v3 = {
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
      selections: v2 /*:: as any*/,
      storageKey: null,
    },
    v4 = {
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
      selections: v2 /*:: as any*/,
      storageKey: null,
    },
    v5 = {
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
      selections: v2 /*:: as any*/,
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v9 = {
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
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        v3 /*:: as any*/,
        v4 /*:: as any*/,
        v5 /*:: as any*/,
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
                v6 /*:: as any*/,
                v7 /*:: as any*/,
                v8 /*:: as any*/,
                v9 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "LibraryPreferenceDefaults",
                  kind: "LinkedField",
                  name: "defaults",
                  plural: false,
                  selections: [
                    v6 /*:: as any*/,
                    v7 /*:: as any*/,
                    v8 /*:: as any*/,
                    v9 /*:: as any*/,
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        v3 /*:: as any*/,
        v4 /*:: as any*/,
        v5 /*:: as any*/,
      ],
    },
    params: {
      id: "e1ab7f2fa99255692c48547aaefd59426c0f3f200bb23c942c2f32161ed37b6e",
      metadata: {},
      name: "libraryBootstrapQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "99b0d26cc06809735b66bbf708c18b53";

export default node;
