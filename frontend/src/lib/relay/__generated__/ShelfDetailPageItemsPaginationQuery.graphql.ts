/**
 * @generated SignedSource<<dc8e3acd2e5b97a700e80baa2a16bc7c>>
 * @relayHash f9efa2fc041b5356e2cd3d2254aea744
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ae6039eac4225af200c2df35eb6b3fe268e80ca40234bd4535a95403e22f0a3f

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfDetailPageItemsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type ShelfDetailPageItemsPaginationQuery$data = {
  readonly fetch__Shelf:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"ShelfDetailPage_items">;
      }
    | null
    | undefined;
};
export type ShelfDetailPageItemsPaginationQuery = {
  response: ShelfDetailPageItemsPaginationQuery$data;
  variables: ShelfDetailPageItemsPaginationQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: 50,
        kind: "LocalArgument",
        name: "count",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "cursor",
      },
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "id",
        variableName: "id",
      },
    ],
    v2 = [
      {
        kind: "Variable",
        name: "after",
        variableName: "cursor",
      },
      {
        kind: "Variable",
        name: "first",
        variableName: "count",
      },
    ],
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "ShelfDetailPageItemsPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "fetch__Shelf",
          plural: false,
          selections: [
            {
              args: [
                {
                  kind: "Variable",
                  name: "count",
                  variableName: "count",
                },
                {
                  kind: "Variable",
                  name: "cursor",
                  variableName: "cursor",
                },
              ],
              kind: "FragmentSpread",
              name: "ShelfDetailPage_items",
            },
          ],
          storageKey: null,
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "ShelfDetailPageItemsPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "fetch__Shelf",
          plural: false,
          selections: [
            {
              alias: null,
              args: v2 /*:: as any*/,
              concreteType: "ManifestationConnection",
              kind: "LinkedField",
              name: "items",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "ManifestationEdge",
                  kind: "LinkedField",
                  name: "edges",
                  plural: true,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "Manifestation",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "__typename",
                          storageKey: null,
                        },
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
              storageKey: null,
            },
            {
              alias: null,
              args: v2 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "ShelfDetailPage_items",
              kind: "LinkedHandle",
              name: "items",
            },
            v3 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "ae6039eac4225af200c2df35eb6b3fe268e80ca40234bd4535a95403e22f0a3f",
      metadata: {},
      name: "ShelfDetailPageItemsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "b0da6e5a328ad2e7beb0fe2283586d18";

export default node;
