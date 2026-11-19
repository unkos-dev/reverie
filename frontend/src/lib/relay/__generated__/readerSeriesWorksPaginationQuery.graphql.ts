/**
 * @generated SignedSource<<40f2b9a1dd44045d3c238f90a873846a>>
 * @relayHash 7f2f38e80847e774b75592a541c9e6be
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID c340ff9c05d0ad064f55ce12190411e54c8da4987da4abb3fd50f77a6a21642c

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeriesWorksPaginationQuery$variables = {
  count?: number | null | undefined;
  cursor?: string | null | undefined;
  id: string;
};
export type readerSeriesWorksPaginationQuery$data = {
  readonly fetch__Series:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"readerSeriesWorks">;
      }
    | null
    | undefined;
};
export type readerSeriesWorksPaginationQuery = {
  response: readerSeriesWorksPaginationQuery$data;
  variables: readerSeriesWorksPaginationQuery$variables;
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
    },
    v4 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "endCursor",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hasNextPage",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      concreteType: "PageInfo",
      kind: "LinkedField",
      name: "pageInfo",
      plural: false,
      selections: [v7 /*:: as any*/, v8 /*:: as any*/],
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerSeriesWorksPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "fetch__Series",
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
              name: "readerSeriesWorks",
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
      name: "readerSeriesWorksPaginationQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "fetch__Series",
          plural: false,
          selections: [
            {
              alias: null,
              args: v2 /*:: as any*/,
              concreteType: "WorkConnection",
              kind: "LinkedField",
              name: "works",
              plural: false,
              selections: [
                {
                  alias: null,
                  args: null,
                  concreteType: "WorkEdge",
                  kind: "LinkedField",
                  name: "edges",
                  plural: true,
                  selections: [
                    {
                      alias: null,
                      args: null,
                      kind: "ScalarField",
                      name: "position",
                      storageKey: null,
                    },
                    {
                      alias: null,
                      args: null,
                      concreteType: "Work",
                      kind: "LinkedField",
                      name: "node",
                      plural: false,
                      selections: [
                        v3 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "title",
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
                                    v5 /*:: as any*/,
                                  ],
                                  storageKey: null,
                                },
                                v6 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v9 /*:: as any*/,
                          ],
                          storageKey: "authors(first:20)",
                        },
                        {
                          alias: null,
                          args: v4 /*:: as any*/,
                          filters: null,
                          handle: "connection",
                          key: "readerWorkAuthors_authors",
                          kind: "LinkedHandle",
                          name: "authors",
                        },
                        {
                          alias: null,
                          args: [
                            {
                              kind: "Literal",
                              name: "first",
                              value: 1,
                            },
                          ],
                          concreteType: "ManifestationConnection",
                          kind: "LinkedField",
                          name: "manifestations",
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
                                      name: "coverUrl",
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
                              args: null,
                              concreteType: "PageInfo",
                              kind: "LinkedField",
                              name: "pageInfo",
                              plural: false,
                              selections: [v8 /*:: as any*/, v7 /*:: as any*/],
                              storageKey: null,
                            },
                          ],
                          storageKey: "manifestations(first:1)",
                        },
                        v5 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v6 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v9 /*:: as any*/,
              ],
              storageKey: null,
            },
            {
              alias: null,
              args: v2 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "readerSeriesWorks_works",
              kind: "LinkedHandle",
              name: "works",
            },
            v3 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "c340ff9c05d0ad064f55ce12190411e54c8da4987da4abb3fd50f77a6a21642c",
      metadata: {},
      name: "readerSeriesWorksPaginationQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "0143a28e663c834b85c34d8c461685bf";

export default node;
