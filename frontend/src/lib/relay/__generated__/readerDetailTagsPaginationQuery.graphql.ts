/**
 * @generated SignedSource<<e1ad7b4aecf4bef0f2531a65125c221e>>
 * @relayHash e2ae8c431505c1faa33a344c8b7b3796
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 3785b2d7ef71c8bc7bf14706bfad83718d00d89ea653e6d2131d9b28be4c860c

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerDetailTagsPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerDetailTagsPaginationQuery$data = {
  readonly node:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerDetailTags">;
      }
    | null
    | undefined;
};
export type readerDetailTagsPaginationQuery = {
  response: readerDetailTagsPaginationQuery$data;
  variables: readerDetailTagsPaginationQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: 20,
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
      name: "readerDetailTagsPaginationQuery",
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
              name: "readerDetailTags",
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
      name: "readerDetailTagsPaginationQuery",
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
                  concreteType: "TagConnection",
                  kind: "LinkedField",
                  name: "tags",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      concreteType: "TagEdge",
                      kind: "LinkedField",
                      name: "edges",
                      plural: true,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "Tag",
                          kind: "LinkedField",
                          name: "node",
                          plural: false,
                          selections: [
                            v3 /*:: as any*/,
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "name",
                              storageKey: null,
                            },
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
                  storageKey: null,
                },
                {
                  alias: null,
                  args: v4 /*:: as any*/,
                  filters: null,
                  handle: "connection",
                  key: "readerDetailTags_tags",
                  kind: "LinkedHandle",
                  name: "tags",
                },
              ],
              type: "Manifestation",
              abstractKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "3785b2d7ef71c8bc7bf14706bfad83718d00d89ea653e6d2131d9b28be4c860c",
      metadata: {},
      name: "readerDetailTagsPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "74d61692c05aab21a4cb8fb4cfba3271";

export default node;
