/**
 * @generated SignedSource<<b4d8204edb954efe1ceb513e2e95e488>>
 * @relayHash fccd739d09611cca661e1e6bcdb38d96
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 6376c263f71769ab20be715fd14933d18d6cb9b1d2bd39c7ac0dc471b8a5b9fc

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
      metadata: null,
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
      id: "6376c263f71769ab20be715fd14933d18d6cb9b1d2bd39c7ac0dc471b8a5b9fc",
      metadata: {},
      name: "readerShelvesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "5fa5b67f8754cf4a24798f70379d9088";

export default node;
