/**
 * @generated SignedSource<<85c0d7f5fbfb2455cba929a47ea34269>>
 * @relayHash 51e85aaeeef4d1f13c5489e6672f6ab3
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0fa22cc4648d2d00854544372009fb20dd0a4af97d4bb8abfe27d24869aa7335

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerAuthorEditingPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerAuthorEditingPaginationQuery$data = {
  readonly node:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerAuthorEditing">;
      }
    | null
    | undefined;
};
export type readerAuthorEditingPaginationQuery = {
  response: readerAuthorEditingPaginationQuery$data;
  variables: readerAuthorEditingPaginationQuery$variables;
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
      name: "readerAuthorEditingPaginationQuery",
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
              name: "readerAuthorEditing",
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
      name: "readerAuthorEditingPaginationQuery",
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
                  args: null,
                  concreteType: "MetadataSnapshot",
                  kind: "LinkedField",
                  name: "metadata",
                  plural: false,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "workId",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "manifestationId",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "etag",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: v4 /*:: as any*/,
                      concreteType: "AuthorConnection",
                      kind: "LinkedField",
                      name: "authors",
                      plural: false,
                      selections: [
                        {
                          alias: null,
                          args: null,
                          concreteType: "AuthorEdge",
                          kind: "LinkedField",
                          name: "edges",
                          plural: true,
                          selections: [
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "cursor",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              concreteType: "Author",
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
                              name: "hasNextPage",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "hasPreviousPage",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "startCursor",
                              storageKey: null,
                            },
                            {
                              alias: null,
                              args: null,
                              kind: "ScalarField",
                              name: "endCursor",
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
                      key: "readerAuthorEditing_authors",
                      kind: "LinkedHandle",
                      name: "authors",
                    },
                  ],
                  storageKey: null,
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
      id: "0fa22cc4648d2d00854544372009fb20dd0a4af97d4bb8abfe27d24869aa7335",
      metadata: {},
      name: "readerAuthorEditingPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "3ebad140745c0e2f2d96c8a54bd30d10";

export default node;
