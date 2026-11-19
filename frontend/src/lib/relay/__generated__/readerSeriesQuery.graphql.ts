/**
 * @generated SignedSource<<c2ece10b088c1b4633310d361411a83c>>
 * @relayHash 7c8150dfd233442626874e63f37ea6dc
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID bc2e21710a9462d54710c1f39cf7f8416cb1aa33e1560e640e6c9ff345b31603

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeriesQuery$variables = {
  id: string;
};
export type readerSeriesQuery$data = {
  readonly series:
    | {
        readonly id: string;
        readonly name: string;
        readonly " $fragmentSpreads": FragmentRefs<"readerSeries">;
      }
    | null
    | undefined;
};
export type readerSeriesQuery = {
  response: readerSeriesQuery$data;
  variables: readerSeriesQuery$variables;
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
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v4 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
      },
    ],
    v5 = [
      {
        kind: "Literal",
        name: "first",
        value: 20,
      },
    ],
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "__typename",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "cursor",
      storageKey: null,
    },
    v8 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "endCursor",
      storageKey: null,
    },
    v9 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hasNextPage",
      storageKey: null,
    },
    v10 = {
      alias: null,
      args: null,
      concreteType: "PageInfo",
      kind: "LinkedField",
      name: "pageInfo",
      plural: false,
      selections: [v8 /*:: as any*/, v9 /*:: as any*/],
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: {
        throwOnFieldError: true,
      },
      name: "readerSeriesQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            v3 /*:: as any*/,
            {
              args: null,
              kind: "FragmentSpread",
              name: "readerSeries",
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
      name: "readerSeriesQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            v2 /*:: as any*/,
            v3 /*:: as any*/,
            {
              alias: null,
              args: v4 /*:: as any*/,
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
                        v2 /*:: as any*/,
                        {
                          alias: null,
                          args: null,
                          kind: "ScalarField",
                          name: "title",
                          storageKey: null,
                        },
                        {
                          alias: null,
                          args: v5 /*:: as any*/,
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
                                    v2 /*:: as any*/,
                                    v3 /*:: as any*/,
                                    v6 /*:: as any*/,
                                  ],
                                  storageKey: null,
                                },
                                v7 /*:: as any*/,
                              ],
                              storageKey: null,
                            },
                            v10 /*:: as any*/,
                          ],
                          storageKey: "authors(first:20)",
                        },
                        {
                          alias: null,
                          args: v5 /*:: as any*/,
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
                                    v2 /*:: as any*/,
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
                              selections: [v9 /*:: as any*/, v8 /*:: as any*/],
                              storageKey: null,
                            },
                          ],
                          storageKey: "manifestations(first:1)",
                        },
                        v6 /*:: as any*/,
                      ],
                      storageKey: null,
                    },
                    v7 /*:: as any*/,
                  ],
                  storageKey: null,
                },
                v10 /*:: as any*/,
              ],
              storageKey: "works(first:50)",
            },
            {
              alias: null,
              args: v4 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "readerSeriesWorks_works",
              kind: "LinkedHandle",
              name: "works",
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "bc2e21710a9462d54710c1f39cf7f8416cb1aa33e1560e640e6c9ff345b31603",
      metadata: {},
      name: "readerSeriesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "57c5d24422f57774c1c377a6f1bf104b";

export default node;
