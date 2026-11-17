/**
 * @generated SignedSource<<6af7faf42e957e64bea0a0a42a6d9c21>>
 * @relayHash 7c7a781401942126ea1d111f7a5ace0a
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID ea6fca60a4413e96cfff068bfdee730040fd7573e436db5dd5c47aed5519f87f

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfItemsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerShelfItemsPaginationQuery$data = {
  readonly node:
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
    v4 = [
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
    ];
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
          concreteType: null,
          kind: "LinkedField",
          name: "node",
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
          concreteType: null,
          kind: "LinkedField",
          name: "node",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            v3 /*:: as any*/,
            {
              kind: "InlineFragment",
              selections: [
                {
                  alias: null,
                  args: v4 /*:: as any*/,
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
                          selections: [v3 /*:: as any*/, v2 /*:: as any*/],
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
                  args: v4 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "readerShelfItems_items",
                  kind: "LinkedHandle",
                  name: "items",
                },
              ],
              type: "Shelf",
              abstractKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "ea6fca60a4413e96cfff068bfdee730040fd7573e436db5dd5c47aed5519f87f",
      metadata: {},
      name: "readerShelfItemsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "6631b1654e5f2c8929baf9f2c41d6ea1";

export default node;
