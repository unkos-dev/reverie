/**
 * @generated SignedSource<<d8a47d9a91012e008a2ca56ac810c1ca>>
 * @relayHash 291cb9a75f76669301ee5359c90f5dcb
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 2aaa0d2c173d8695a56ffb607ca29e676f635123cb78101967445a1903d6b86d

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type SeriesPageQuery$variables = {
  id: string;
};
export type SeriesPageQuery$data = {
  readonly series:
    | {
        readonly " $fragmentSpreads": FragmentRefs<"SeriesPage_series">;
      }
    | null
    | undefined;
};
export type SeriesPageQuery = {
  response: SeriesPageQuery$data;
  variables: SeriesPageQuery$variables;
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
      name: "name",
      storageKey: null,
    },
    v3 = [
      {
        kind: "Literal",
        name: "first",
        value: 50,
      },
    ],
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
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
      name: "SeriesPageQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "Series",
          kind: "LinkedField",
          name: "series",
          plural: false,
          selections: [
            {
              args: null,
              kind: "FragmentSpread",
              name: "SeriesPage_series",
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
      name: "SeriesPageQuery",
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
            {
              alias: null,
              args: v3 /*:: as any*/,
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
                        v4 /*:: as any*/,
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
                                    v4 /*:: as any*/,
                                    v2 /*:: as any*/,
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
                          key: "SeriesPage_authors",
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
                                    v4 /*:: as any*/,
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
              args: v3 /*:: as any*/,
              filters: null,
              handle: "connection",
              key: "SeriesPage_works",
              kind: "LinkedHandle",
              name: "works",
            },
            v4 /*:: as any*/,
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "2aaa0d2c173d8695a56ffb607ca29e676f635123cb78101967445a1903d6b86d",
      metadata: {},
      name: "SeriesPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "b04cf7cb836e8b42976665e38e823d78";

export default node;
