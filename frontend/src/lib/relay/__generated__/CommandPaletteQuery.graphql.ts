/**
 * @generated SignedSource<<d900b5a3c316c05e9177dfdeb5229bfb>>
 * @relayHash 1303d57aa449acf525f37afc195997bd
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 4263f0ad8e894f31bafa49df4e334bc15c3c75fdf8ffc67c215ef16058ce9d32

import { ConcreteRequest } from "relay-runtime";
export type CommandPaletteQuery$variables = {
  q: string;
};
export type CommandPaletteQuery$data = {
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
export type CommandPaletteQuery = {
  response: CommandPaletteQuery$data;
  variables: CommandPaletteQuery$variables;
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
      metadata: {
        throwOnFieldError: true,
      },
      name: "CommandPaletteQuery",
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
      name: "CommandPaletteQuery",
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
      id: "4263f0ad8e894f31bafa49df4e334bc15c3c75fdf8ffc67c215ef16058ce9d32",
      metadata: {},
      name: "CommandPaletteQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "2dfc0467aa4d3cf221d7fa0801248e82";

export default node;
