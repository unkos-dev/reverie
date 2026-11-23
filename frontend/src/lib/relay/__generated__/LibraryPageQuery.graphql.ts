/**
 * @generated SignedSource<<37d1668f127beba02286f99c1551868b>>
 * @relayHash 5f978093fccd3210a21560bacac77745
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID df07c06f21c9483829a419937266f20f94c883ac04dee03abd0f631da8f87808

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type LibraryPageQuery$variables = {
  authorIds: ReadonlyArray<string>;
  seriesIds: ReadonlyArray<string>;
  shelfIds: ReadonlyArray<string>;
};
export type LibraryPageQuery$data = {
  readonly viewer: {
    readonly id: string;
    readonly libraryPreferences: {
      readonly " $fragmentSpreads": FragmentRefs<"useLibraryPreferences_preferences">;
    };
  };
  readonly " $fragmentSpreads": FragmentRefs<
    "LibraryPage_labels" | "TypeaheadMultiSelect_suggestions" | "readerShelves"
  >;
};
export type LibraryPageQuery = {
  response: LibraryPageQuery$data;
  variables: LibraryPageQuery$variables;
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
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hiddenColumns",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "density",
      storageKey: null,
    },
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "view",
      storageKey: null,
    },
    v6 = {
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
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v8 = [v1 /*:: as any*/, v7 /*:: as any*/],
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "itemCount",
      storageKey: null,
    },
    v10 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "LibraryPageQuery",
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
        {
          args: [
            {
              kind: "Variable",
              name: "authorIds",
              variableName: "authorIds",
            },
            {
              kind: "Variable",
              name: "seriesIds",
              variableName: "seriesIds",
            },
            {
              kind: "Variable",
              name: "shelfIds",
              variableName: "shelfIds",
            },
          ],
          kind: "FragmentSpread",
          name: "LibraryPage_labels",
        },
        {
          args: null,
          kind: "FragmentSpread",
          name: "readerShelves",
        },
        {
          args: null,
          kind: "FragmentSpread",
          name: "TypeaheadMultiSelect_suggestions",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "LibraryPageQuery",
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
                v2 /*:: as any*/,
                v3 /*:: as any*/,
                v4 /*:: as any*/,
                v5 /*:: as any*/,
                v6 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "LibraryPreferenceDefaults",
                  kind: "LinkedField",
                  name: "defaults",
                  plural: false,
                  selections: [
                    v3 /*:: as any*/,
                    v4 /*:: as any*/,
                    v5 /*:: as any*/,
                    v6 /*:: as any*/,
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
        {
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
          selections: v8 /*:: as any*/,
          storageKey: null,
        },
        {
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
          selections: v8 /*:: as any*/,
          storageKey: null,
        },
        {
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
          selections: [v1 /*:: as any*/, v7 /*:: as any*/, v9 /*:: as any*/],
          storageKey: null,
        },
        {
          alias: null,
          args: v10 /*:: as any*/,
          concreteType: "ShelfConnection",
          kind: "LinkedField",
          name: "shelves",
          plural: false,
          selections: [
            {
              alias: null,
              args: null,
              concreteType: "ShelfEdge",
              kind: "LinkedField",
              name: "edges",
              plural: true,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "Shelf",
                  kind: "LinkedField",
                  name: "node",
                  plural: false,
                  selections: [
                    v1 /*:: as any*/,
                    v7 /*:: as any*/,
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "isSystem",
                      storageKey: null,
                    },
                    v9 /*:: as any*/,
                    v2 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "cursor",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              concreteType: "PageInfo",
              kind: "LinkedField",
              name: "pageInfo",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "endCursor",
                  storageKey: null,
                },
                {
                  alias: null,
                  args: null,
                  kind: "ScalarField",
                  name: "hasNextPage",
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: "shelves(first:50)",
        },
        {
          alias: null,
          args: v10 /*:: as any*/,
          filters: null,
          handle: "connection",
          key: "readerShelves_shelves",
          kind: "LinkedHandle",
          name: "shelves",
        },
      ],
    },
    params: {
      id: "df07c06f21c9483829a419937266f20f94c883ac04dee03abd0f631da8f87808",
      metadata: {},
      name: "LibraryPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "2edf78d7345d266b880d952e140e6922";

export default node;
