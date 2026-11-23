/**
 * @generated SignedSource<<43c442086c792200f9e9b2bfa8a6eb42>>
 * @relayHash 11cf3373c8dd608c67cf893f11ceb1be
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID f62a7246413980413e46be70dc316003cdd66d5cb6561e8c9b143803a1046bd2

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerShelvesQuery$variables = Record<PropertyKey, never>;
export type readerShelvesQuery$data = {
  readonly " $fragmentSpreads": FragmentRefs<"readerShelves">;
};
export type readerShelvesQuery = {
  response: readerShelvesQuery$data;
  variables: readerShelvesQuery$variables;
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
      name: "readerShelvesQuery",
      selections: [
        {
          args: null,
          kind: "FragmentSpread",
          name: "readerShelves",
        },
      ],
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [],
      kind: "Operation",
      name: "readerShelvesQuery",
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
          key: "readerShelves_shelves",
          kind: "LinkedHandle",
          name: "shelves",
        },
      ],
    },
    params: {
      id: "f62a7246413980413e46be70dc316003cdd66d5cb6561e8c9b143803a1046bd2",
      metadata: {},
      name: "readerShelvesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "52a39884f4750f238115c15376267de5";

export default node;
