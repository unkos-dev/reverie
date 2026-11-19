/**
 * @generated SignedSource<<80a45785a7e3d2a26b8058e3e6112ab9>>
 * @relayHash bc7dd8d98d9805c686fea03bad6ddd61
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 37d9b9f35933e7b9b3592140e3a1a42be48ac404b7a1e932b2e1db91e4bffb0f

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelfQuery$variables = {
  id: string;
};
export type readerShelfQuery$data = {
  readonly shelf:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerShelf">;
      }
    | null
    | undefined;
};
export type readerShelfQuery = {
  response: readerShelfQuery$data;
  variables: readerShelfQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
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
      name: "id",
      storageKey: null,
    },
    v3 = [
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
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "readerShelf",
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
      name: "readerShelfQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Shelf",
          kind: "LinkedField",
          name: "shelf",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "name",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "isSystem",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "itemCount",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "createdAt",
              storageKey: null,
            },
            {
              alias: null,
              args: null,
              kind: "ScalarField",
              name: "updatedAt",
              storageKey: null,
            },
            {
              alias: null,
              args: v3 /*:: as any*/,
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
                        v2 /*:: as any*/,
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
              storageKey: "items(first:50)",
            },
            {
              alias: null,
              args: v3 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "readerShelfItems_items",
              kind: "LinkedHandle",
              name: "items",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "37d9b9f35933e7b9b3592140e3a1a42be48ac404b7a1e932b2e1db91e4bffb0f",
      metadata: {},
      name: "readerShelfQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "aeea27ff93daea1d3a3cf8a66a129cab";

export default node;
