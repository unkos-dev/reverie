/**
 * @generated SignedSource<<1220f3069a0dad32afd5e1858db99da9>>
 * @relayHash 9a235f9ccdbf500f57582277f5723c85
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID dca5376c4d949cebb82377c49614c9aae801309feaf99fee32284a4b365c93c7

import { ConcreteRequest } from "relay-runtime";
export type readerSearchQuery$variables = {
  q: string;
};
export type readerSearchQuery$data = {
  readonly search: ReadonlyArray<{
    readonly manifestation:
      | {
          readonly id: string;
          readonly work:
            | {
                readonly authors: {
                  readonly edges: ReadonlyArray<{
                    readonly node: {
                      readonly name: string;
                    };
                  }>;
                  readonly pageInfo: {
                    readonly endCursor: string | null | undefined;
                    readonly hasNextPage: boolean;
                  };
                };
                readonly title: string;
              }
            | null
            | undefined;
        }
      | null
      | undefined;
    readonly snippet: string | null | undefined;
  }>;
};
export type readerSearchQuery = {
  response: readerSearchQuery$data;
  variables: readerSearchQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "q",
      },
    ],
    v1 = [
      {
        kind: "Variable",
        name: "q",
        variableName: "q",
      },
    ],
    v2 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "snippet",
      storageKey: null,
    },
    v3 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v4 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "title",
      storageKey: null,
    },
    v5 = [
      {
        kind: "Literal",
        name: "first",
        value: 5,
      },
    ],
    v6 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "name",
      storageKey: null,
    },
    v7 = {
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
          name: "endCursor",
          storageKey: null,
        },
      ],
      storageKey: null,
    };
  return {
    fragment: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerSearchQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Manifestation",
              kind: "LinkedField",
              name: "manifestation",
              plural: false,
              selections: [
                v3 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Work",
                  kind: "LinkedField",
                  name: "work",
                  plural: false,
                  selections: [
                    v4 /*:: as any*/,
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
                              selections: [v6 /*:: as any*/],
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v7 /*:: as any*/,
                      ],
                      storageKey: "authors(first:5)",
                    },
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
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
      name: "readerSearchQuery",
      selections: [
        {
          alias: null,
          args: v1 /*:: as any*/,
          concreteType: "SearchHit",
          kind: "LinkedField",
          name: "search",
          plural: true,
          selections: [
            v2 /*:: as any*/,
            {
              alias: null,
              args: null,
              concreteType: "Manifestation",
              kind: "LinkedField",
              name: "manifestation",
              plural: false,
              selections: [
                v3 /*:: as any*/,
                {
                  alias: null,
                  args: null,
                  concreteType: "Work",
                  kind: "LinkedField",
                  name: "work",
                  plural: false,
                  selections: [
                    v4 /*:: as any*/,
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
                              selections: [v6 /*:: as any*/, v3 /*:: as any*/],
                              storageKey: null,
                            },
                          ],
                          storageKey: null,
                        },
                        v7 /*:: as any*/,
                      ],
                      storageKey: "authors(first:5)",
                    },
                    v3 /*:: as any*/,
                  ],
                  storageKey: null,
                },
              ],
              storageKey: null,
            },
          ],
          storageKey: null,
        },
      ],
    },
    params: {
      id: "dca5376c4d949cebb82377c49614c9aae801309feaf99fee32284a4b365c93c7",
      metadata: {},
      name: "readerSearchQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "2fcfa3b0cce7837f970178b7d4637782";

export default node;
