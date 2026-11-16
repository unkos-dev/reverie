/**
 * @generated SignedSource<<451276ec3dfdb10b86a600b6df796f46>>
 * @relayHash 3b19076ad9ddcbd85943465734aa4d26
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID f0f870b50efb58cbab7c1f4aa8b2c0c95843fec719ad87bdbb3d7db7e83650dc

import { ConcreteRequest } from "relay-runtime";
export type mutationsAuthorPageQuery$variables = {
  after?: string | null | undefined;
  first: number;
  id: string;
};
export type mutationsAuthorPageQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly metadata: {
          readonly authors: {
            readonly edges: ReadonlyArray<{
              readonly cursor: string;
              readonly node: {
                readonly id: string;
                readonly name: string;
              };
            }>;
            readonly pageInfo: {
              readonly endCursor: string | null | undefined;
              readonly hasNextPage: boolean;
              readonly hasPreviousPage: boolean;
              readonly startCursor: string | null | undefined;
            };
          };
          readonly etag: string;
          readonly manifestationId: string;
          readonly workId: string;
        };
      }
    | null
    | undefined;
};
export type mutationsAuthorPageQuery = {
  response: mutationsAuthorPageQuery$data;
  variables: mutationsAuthorPageQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "after",
    },
    v1 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "first",
    },
    v2 = {
      defaultValue: null,
      kind: "LocalArgument",
      name: "id",
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
        alias: null,
        args: [
          {
            kind: "Variable",
            name: "id",
            variableName: "id",
          },
        ],
        concreteType: "Manifestation",
        kind: "LinkedField",
        name: "manifestation",
        plural: false,
        selections: [
          v3 /*:: as any*/,
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
                args: [
                  {
                    kind: "Variable",
                    name: "after",
                    variableName: "after",
                  },
                  {
                    kind: "Variable",
                    name: "first",
                    variableName: "first",
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
            ],
            storageKey: null,
          },
        ],
        storageKey: null,
      },
    ];
  return {
    fragment: {
      argumentDefinitions: [v0 /*:: as any*/, v1 /*:: as any*/, v2 /*:: as any*/],
      kind: "Fragment",
      metadata: null,
      name: "mutationsAuthorPageQuery",
      selections: v4 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: [v2 /*:: as any*/, v1 /*:: as any*/, v0 /*:: as any*/],
      kind: "Operation",
      name: "mutationsAuthorPageQuery",
      selections: v4 /*:: as any*/,
    },
    params: {
      id: "f0f870b50efb58cbab7c1f4aa8b2c0c95843fec719ad87bdbb3d7db7e83650dc",
      metadata: {},
      name: "mutationsAuthorPageQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "a548b3f522d5f721c1d0ff4295ef16cb";

export default node;
