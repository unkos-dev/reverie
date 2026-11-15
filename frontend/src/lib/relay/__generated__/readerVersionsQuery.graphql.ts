/**
 * @generated SignedSource<<956f40ba4c19763218c97764e493a838>>
 * @relayHash c88fa7123df50e6b2da61428b404c3ab
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID a02bdaf67417f1d853bdf86e94ecf4abb32547bc394bfad19aebefecc58496e1

import { ConcreteRequest } from "relay-runtime";
export type readerVersionsQuery$variables = {
  id: string;
};
export type readerVersionsQuery$data = {
  readonly manifestation:
    | {
        readonly id: string;
        readonly versions: {
          readonly acceptedCount: number;
          readonly pending: {
            readonly edges: ReadonlyArray<{
              readonly cursor: string;
              readonly node: {
                readonly confidenceScore: number;
                readonly fieldName: string;
                readonly id: string;
                readonly matchType: string;
                readonly newValue: any;
                readonly observationCount: number;
                readonly source: string;
                readonly status: string;
              };
            }>;
            readonly pageInfo: {
              readonly endCursor: string | null | undefined;
              readonly hasNextPage: boolean;
              readonly hasPreviousPage: boolean;
              readonly startCursor: string | null | undefined;
            };
          };
          readonly pendingCount: number;
        };
      }
    | null
    | undefined;
};
export type readerVersionsQuery = {
  response: readerVersionsQuery$data;
  variables: readerVersionsQuery$variables;
};

const node: ConcreteRequest = (function () {
  var v0 = [
      {
        defaultValue: null,
        kind: "LocalArgument",
        name: "id",
      },
    ],
    v1 = {
      alias: null,
      args: null,
      kind: "ScalarField",
      name: "id",
      storageKey: null,
    },
    v2 = [
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
          v1 /*:: as any*/,
          {
            alias: null,
            args: null,
            concreteType: "MetadataVersions",
            kind: "LinkedField",
            name: "versions",
            plural: false,
            selections: [
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "pendingCount",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                kind: "ScalarField",
                name: "acceptedCount",
                storageKey: null,
              },
              {
                alias: null,
                args: null,
                concreteType: "MetadataVersionConnection",
                kind: "LinkedField",
                name: "pending",
                plural: false,
                selections: [
                  {
                    alias: null,
                    args: null,
                    concreteType: "MetadataVersionEdge",
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
                        concreteType: "MetadataVersion",
                        kind: "LinkedField",
                        name: "node",
                        plural: false,
                        selections: [
                          v1 /*:: as any*/,
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "fieldName",
                            storageKey: null,
                          },
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "source",
                            storageKey: null,
                          },
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "newValue",
                            storageKey: null,
                          },
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "status",
                            storageKey: null,
                          },
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "confidenceScore",
                            storageKey: null,
                          },
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "matchType",
                            storageKey: null,
                          },
                          {
                            alias: null,
                            args: null,
                            kind: "ScalarField",
                            name: "observationCount",
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
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Fragment",
      metadata: null,
      name: "readerVersionsQuery",
      selections: v2 /*:: as any*/,
      type: "Query",
      abstractKey: null,
    },
    kind: "Request",
    operation: {
      argumentDefinitions: v0 /*:: as any*/,
      kind: "Operation",
      name: "readerVersionsQuery",
      selections: v2 /*:: as any*/,
    },
    params: {
      id: "a02bdaf67417f1d853bdf86e94ecf4abb32547bc394bfad19aebefecc58496e1",
      metadata: {},
      name: "readerVersionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "bbc6d3d3528b2af271095e5321df77c1";

export default node;
