/**
 * @generated SignedSource<<66a8c5a79b68f637d97072c86ded70c8>>
 * @relayHash 23b0a9f19b3e85351fedd950c3d86ad4
 * @lightSyntaxTransform
 */

/* tslint:disable */
/* eslint-disable */
// @ts-nocheck

// @relayRequestID 0e73ed04932a76fcdd13bb12740f66d8ce6299c34197033b94c2ed7ea63af1d4

import { ConcreteRequest } from "relay-runtime";
export type readerVersionsQuery$variables = {
  count?: number | null | undefined;
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
        defaultValue: 20,
        kind: "LocalArgument",
        name: "count",
      },
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
                args: [
                  {
                    kind: "Variable",
                    name: "first",
                    variableName: "count",
                  },
                ],
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
      id: "0e73ed04932a76fcdd13bb12740f66d8ce6299c34197033b94c2ed7ea63af1d4",
      metadata: {},
      name: "readerVersionsQuery",
      operationKind: "query",
      text: null,
    },
  };
})();

(node as any).hash = "d8c5ee1be869dfbb849860c34db029e3";

export default node;
