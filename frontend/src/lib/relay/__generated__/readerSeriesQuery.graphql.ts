/**
 * @generated SignedSource<<04c51a5c2fd339648fe676230c4a6d0e>>
 * @relayHash 53b5a676fe955dceded6b90f622e65e9
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 9e68ef7303a1442db8316c31bb8d2f048c7019eb38dd1e34b89abc47dd48143d

import { ConcreteRequest } from "relay-runtime";
import { FragmentRefs } from "relay-runtime";
export type readerSeriesQuery$variables = {
  id: string;
};
export type readerSeriesQuery$data = {
  readonly series:
    | {
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
    v5 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "hasNextPage",
      storageKey: null,
    },
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "endCursor",
      storageKey: null,
    },
    v7 = {
      alias: null,
      args: null,
      concreteType: "PageInfo",
      kind: "LinkedField",
      name: "pageInfo",
      plural: false,
      selections: [v5 /*:: as any*/, v6 /*:: as any*/],
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
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
                          args: [
                            {
                              kind: "Literal",
                              name: "first",
                              value: 5,
                            },
                          ],
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
                                  selections: [v3 /*:: as any*/, v2 /*:: as any*/],
                                  storageKey: null,
                                },
                              ],
                              storageKey: null,
                            },
                            v7 /*:: as any*/,
                          ],
                          storageKey: "authors(first:5)",
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
                            v7 /*:: as any*/,
                          ],
                          storageKey: "manifestations(first:1)",
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
                  selections: [v6 /*:: as any*/, v5 /*:: as any*/],
                  storageKey: null,
                },
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
      id: "9e68ef7303a1442db8316c31bb8d2f048c7019eb38dd1e34b89abc47dd48143d",
      metadata: {},
      name: "readerSeriesQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "c2506439909db97799618f3b5e2c5a98";

export default node;
