/**
 * @generated SignedSource<<8ab16f3c868206e5ad0ca8cd520af028>>
 * @relayHash b1f663458fe60aebd956c21fdbc20959
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 4cb898a2c018d410cc2673767b6350cc63b6a43e119c86c0c343396e110b8eee

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type ShelvesListPageQuery$variables = Record<PropertyKey, never>;
export type ShelvesListPageQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"ShelvesListPage_shelves">;
};
export type ShelvesListPageQuery = {
  response: ShelvesListPageQuery$data;
  variables: ShelvesListPageQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
    {
      kind: "Literal",
      name: "first",
      value: 50,
    },
  ];
  return {
    fragment: {
      argumentDefinitions: [],
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "ShelvesListPageQuery",
      selections: [
        {
          args: null,
          kind: "FragmentSpread",
          name: "ShelvesListPage_shelves",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [],
      kind: "Operation",
      name: "ShelvesListPageQuery",
      selections: [
        {
          alias: null,
          args: v0 /*:: as any*/,
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
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "id",
                      storageKey: null,
                    },
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
          storageKey: "shelves(first:50)",
        },
        {
          alias: null,
          args: v0 /*:: as any*/,
          filters: null,
          handle: "connection",
          key: "ShelvesListPage_shelves",
          kind: "LinkedHandle",
          name: "shelves",
        },
      ],
    },
    params: {
      id: "4cb898a2c018d410cc2673767b6350cc63b6a43e119c86c0c343396e110b8eee",
      metadata: {},
      name: "ShelvesListPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "5d57a6925b45c430ce5077877b474616";

export default node;
