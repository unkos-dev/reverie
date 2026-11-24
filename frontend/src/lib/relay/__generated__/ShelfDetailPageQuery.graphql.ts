/**
 * @generated SignedSource<<267712a1915ad3e9348fe9410aa7911d>>
 * @relayHash 6f7cc84edfe535a938c7e481b84ade79
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 917a17e3c06d980914327b9edd473f0bd252776e87caa89fbe159baccc2d1714

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelfDetailPageQuery$variables = {
  id: string;
};
export type ShelfDetailPageQuery$data = {
  readonly shelf:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"ShelfDetailPage_shelf">;
      }
    | null
    | undefined;
};
export type ShelfDetailPageQuery = {
  response: ShelfDetailPageQuery$data;
  variables: ShelfDetailPageQuery$variables;
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
    v2 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
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
      metadata: {
        throwOnFieldError: true,
      },
      name: "ShelfDetailPageQuery",
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
              name: "ShelfDetailPage_shelf",
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
      name: "ShelfDetailPageQuery",
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
              name: "itemCount",
              storageKey: null,
            },
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
              storageKey: "items(first:50)",
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
      id: "917a17e3c06d980914327b9edd473f0bd252776e87caa89fbe159baccc2d1714",
      metadata: {},
      name: "ShelfDetailPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "ae09dcc2b30b910c54d4abfd428d1bea";

export default node;
