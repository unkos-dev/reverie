/**
 * @generated SignedSource<<7a37d58f1b3e673760cb3911d82695d0>>
 * @relayHash 60fff003847e485bb016123be1f7b891
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 206e0d975d0edc943a56ddab8fdd0f3f2353d6e6f024258876b4de3d549eb40c

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfItemsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerShelfItemsPaginationQuery$data = {
  readonly fetch__Shelf:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerShelfItems">;
      }
    | null
    | undefined;
};
export type readerShelfItemsPaginationQuery = {
  response: readerShelfItemsPaginationQuery$data;
  variables: readerShelfItemsPaginationQuery$variables;
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
      name: "readerShelfItemsPaginationQuery",
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
              name: "readerShelfItems",
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
      name: "readerShelfItemsPaginationQuery",
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
              key: "readerShelfItems_items",
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
      id: "206e0d975d0edc943a56ddab8fdd0f3f2353d6e6f024258876b4de3d549eb40c",
      metadata: {},
      name: "readerShelfItemsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "f366d894eff54bc4877dd3881a7fb59f";

export default node;
